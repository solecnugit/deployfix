use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    num::NonZeroUsize,
};

use log::error;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while},
    character::complete::{char, multispace0},
    combinator::{map, opt},
    multi::{separated_list0, separated_list1},
    sequence::{delimited, preceded, tuple},
    IResult,
};
use thiserror::Error;

use crate::util;

use super::{
    Entity, EntityName, EntityRule, EntityRuleMetadata, EntityRuleSource, EntityRuleType,
    EntitySource,
};

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("YAML error: {0}")]
    YamlError(#[from] serde_yaml::Error),
    #[error("DeployIR error: {0}")]
    DeployIRError(String),
    #[error("Unknown error: {0}")]
    CustomError(String),
}

pub trait Parser {
    fn parse(&self, data: &str, source: EntitySource) -> Result<Vec<Entity>, ParserError>;
}

pub struct JsonParser;
pub struct YamlParser;
pub struct DeployIRParser;
pub struct NomDeployIRParser;

impl JsonParser {
    pub fn new() -> Self {
        Self
    }
}

impl Parser for JsonParser {
    fn parse(&self, data: &str, source: EntitySource) -> Result<Vec<Entity>, ParserError> {
        let entities: Vec<Entity> = serde_json::from_str(data)?;
        Ok(entities
            .into_iter()
            .map(|mut e| {
                e.set_source(source.clone());
                e
            })
            .collect())
    }
}

impl YamlParser {
    pub fn new() -> Self {
        Self
    }
}

impl Parser for YamlParser {
    fn parse(&self, data: &str, source: EntitySource) -> Result<Vec<Entity>, ParserError> {
        let entities: Vec<Entity> = serde_yaml::from_str(data)?;
        Ok(entities
            .into_iter()
            .map(|mut e| {
                e.set_source(source.clone());
                e
            })
            .collect())
    }
}

impl DeployIRParser {
    pub fn new() -> Self {
        Self
    }

    fn parse_rule_metadata(
        &self,
        rule: &str,
        default_file: &str,
        default_line: usize,
    ) -> Result<EntityRuleMetadata, ParserError> {
        let parts: Vec<&str> = rule.split(';').collect();
        let mut map = parts
            .iter()
            .filter_map(|p| {
                let parts: Vec<&str> = p.split('=').collect();

                if parts.len() != 2 {
                    return None;
                }

                Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
            })
            .collect::<BTreeMap<String, String>>();

        let file = map
            .get("File")
            .map(|e| e.to_string())
            .or_else(|| Some(default_file.to_string()));

        let line = map
            .get("Line")
            .map(|e| e.parse().unwrap())
            .or_else(|| NonZeroUsize::new(default_line));

        map.remove("File");
        map.remove("Line");

        if !map.is_empty() {
            Ok(EntityRuleMetadata::new(file, line, Some(map)))
        } else {
            Ok(EntityRuleMetadata::new(file, line, None))
        }
    }

    fn parse_rule(
        &self,
        name: &str,
        rule: &str,
        r#type: EntityRuleType,
        metadata: Option<EntityRuleMetadata>,
        source: EntityRuleSource,
    ) -> Result<EntityRule, ParserError> {
        let name = EntityName(name.to_string());

        if rule.contains(';') {
            let targets: BTreeSet<EntityName> =
                rule.split(',').map(|e| EntityName(e.to_string())).collect();

            Ok(EntityRule::multi(name, targets, r#type, source, metadata))
        } else {
            let target = EntityName(rule.to_string());

            Ok(EntityRule::mono(name, target, r#type, source, metadata))
        }
    }
}

impl Parser for DeployIRParser {
    /*
       DeployIR File Format
       ----------------
       A require B // File=foo.ir, Line=1
       A conflict C // File=foo.ir, Line=2
       A require B;C;D // File=foo.ir, Line=3
       A conflict B;C;D
    */
    fn parse(&self, data: &str, entity_source: EntitySource) -> Result<Vec<Entity>, ParserError> {
        let mut entities: HashMap<&str, Entity> = HashMap::new();
        let entity_path = entity_source.as_ref();
        for (idx, line) in data.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            let rule_source = EntityRuleSource::new(entity_source.as_ref(), idx + 1);

            let (rule_part, rule_metadata) = if line.contains("//") {
                let parts: Vec<&str> = line.split("//").collect();
                let rule = parts[0].trim();
                let source_rule = parts[1].trim();

                let rule_metadata = self.parse_rule_metadata(source_rule, entity_path, idx + 1)?;

                (rule, Some(rule_metadata))
            } else {
                (line, None)
            };

            let mut parts = rule_part.split_whitespace();
            let name = parts
                .next()
                .ok_or_else(|| ParserError::DeployIRError(format!("Invalid line: {}", line)))?;
            let op = parts
                .next()
                .ok_or_else(|| ParserError::DeployIRError(format!("Invalid line: {}", line)))?;
            let rule = parts
                .next()
                .ok_or_else(|| ParserError::DeployIRError(format!("Invalid line: {}", line)))?;

            let r#type = match op {
                "require" => EntityRuleType::Require,
                "exclude" => EntityRuleType::Exclude,
                _ => {
                    return Err(ParserError::DeployIRError(format!(
                        "Invalid operation: {}",
                        op
                    )))
                }
            };

            let entity = entities.entry(name).or_insert_with(|| Entity::new(name));
            let rule = self.parse_rule(name, rule, r#type.clone(), rule_metadata, rule_source)?;

            match r#type {
                EntityRuleType::Require => entity.add_require(rule),
                EntityRuleType::Exclude => entity.add_exclude(rule),
            }
        }

        Ok(entities
            .into_values()
            .map(|mut e| {
                e.set_source(entity_source.clone());
                e
            })
            .collect())
    }
}

pub fn get_parser(format: &str) -> Result<Box<dyn Parser>, ParserError> {
    match format {
        "json" => Ok(Box::new(JsonParser::new())),
        "yaml" => Ok(Box::new(YamlParser::new())),
        "deployfix" => Ok(Box::new(NomDeployIRParser::new())),
        _ => Err(ParserError::CustomError(format!(
            "Unknown format: {}",
            format
        ))),
    }
}

impl NomDeployIRParser {
    pub fn new() -> Self {
        Self
    }

    fn parse_op(line: &str) -> IResult<&str, EntityRuleType> {
        alt((
            map(tag("require"), |_| EntityRuleType::Require),
            map(tag("exclude"), |_| EntityRuleType::Exclude),
        ))(line)
    }

    fn parse_item(line: &str) -> IResult<&str, String> {
        let (rest, name) = preceded(multispace0, take_until(" "))(line)?;

        Ok((rest, name.to_string()))
    }

    fn parse_entity_item(line: &str) -> IResult<&str, String> {
        let (rest, name) = preceded(multispace0, take_while(|ch| ch != ',' && ch != ' '))(line)?;

        Ok((rest, name.to_string()))
    }

    fn parse_entity_name(line: &str) -> IResult<&str, EntityName> {
        let (rest, name) = Self::parse_item(line)?;

        Ok((rest, EntityName(name)))
    }

    fn parse_target_entities(line: &str) -> IResult<&str, BTreeSet<String>> {
        let (rest, names) = separated_list1(char(','), Self::parse_entity_item)(line)?;

        Ok((rest, names.into_iter().collect()))
    }

    fn parse_metadata_entry(line: &str) -> IResult<&str, (String, String)> {
        let (rest, (key, _, value)) = tuple((
            preceded(multispace0, take_until("=")),
            preceded(multispace0, char('=')),
            preceded(multispace0, take_until(";")),
        ))(line)?;

        Ok((rest, (key.to_string(), value.to_string())))
    }

    fn parse_metadata(line: &str) -> IResult<&str, Option<EntityRuleMetadata>> {
        let (rest, mut metadata) = opt(delimited(
            tag("//"),
            map(
                separated_list0(char(';'), Self::parse_metadata_entry),
                |entries| {
                    let mut map = BTreeMap::new();
                    for (k, v) in entries {
                        map.insert(k, v);
                    }
                    map
                },
            ),
            char(';'),
        ))(line)?;

        let mut metadata = match metadata {
            Some(m) => m,
            None => return Ok((rest, None)),
        };

        let file = metadata.remove("file").map(|e| e.to_string());
        let line = metadata.remove("line").map(|e| e.parse().unwrap());

        let map = if metadata.is_empty() {
            None
        } else {
            Some(metadata)
        };

        if file.is_none() && line.is_none() && map.is_none() {
            return Ok((rest, None));
        }

        let metadata = EntityRuleMetadata::new(file, line, map);

        Ok((rest, Some(metadata)))
    }

    fn parse_rule<'a>(
        line: &'a str,
        source: &EntitySource,
        line_num: usize,
    ) -> IResult<&'a str, EntityRule> {
        let (rest, (name, op, target, metadata)) = tuple((
            preceded(multispace0, Self::parse_entity_name),
            preceded(multispace0, Self::parse_op),
            preceded(multispace0, Self::parse_target_entities),
            preceded(multispace0, Self::parse_metadata),
        ))(line)?;

        let source = EntityRuleSource::File(source.as_ref().to_string(), line_num);
        let rule = match target.len() {
            0 => unreachable!(),
            1 => {
                let target = target.into_iter().next().unwrap();
                let target = EntityName(target);
                EntityRule::mono(name, target, op, source, metadata)
            }
            _ => {
                let target = target.into_iter().map(EntityName).collect();
                EntityRule::multi(name, target, op, source, metadata)
            }
        };

        Ok((rest, rule))
    }
}

impl Parser for NomDeployIRParser {
    fn parse(&self, data: &str, source: EntitySource) -> Result<Vec<Entity>, ParserError> {
        let rules = data
            .lines()
            .enumerate()
            .map(|(idx, line)| (idx, Self::parse_rule(line, &source, idx + 1)))
            .collect::<Vec<_>>();

        let errs = rules
            .iter()
            .filter_map(|(i, r)| match r {
                Ok(_) => None,
                Err(e) => Some(format!("Line {}: {}", i + 1, e)),
            })
            .collect::<Vec<_>>();

        if !errs.is_empty() {
            return Err(ParserError::DeployIRError(errs.join("\n")));
        }

        let rules = rules
            .into_iter()
            .filter_map(|(i, r)| r.ok().map(|(res, rule)| (i, res, rule)))
            .map(|(i, rest, rule)| {
                if !rest.is_empty() {
                    error!("Line {}: Unparsed: {}", i + 1, rest);
                }

                rule
            })
            .collect::<Vec<_>>();

        let entities = util::rule_set_to_entity_set(rules);

        Ok(entities)
    }
}
