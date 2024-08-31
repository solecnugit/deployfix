use std::fmt::Display;

use super::{Entity, EntityRule, EntityRuleMetadata};

pub struct DeployIRFormatter<'a>(&'a Vec<Entity>);

impl<'a> Display for DeployIRFormatter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for entity in self.0.iter() {
            self.write_entity(entity, f)?;
        }
        Ok(())
    }
}

impl<'a> DeployIRFormatter<'a> {
    /*
       Format:
       A require B // File=podA.yaml;Line=1
       B require C
       C require D
       A conflict D

       B require Q // File=podB.yaml;Line=1
       Q require A // File=podQ.yaml;Line=1
    */

    fn write_metadata(
        &self,
        metadata: &EntityRuleMetadata,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(
            f,
            "// File={};Line={};",
            metadata.file().unwrap_or("unknown"),
            metadata.line().unwrap_or(0)
        )?;

        if let Some(metadata) = metadata.get_metadata() {
            for (key, value) in metadata.iter() {
                write!(f, "{}={};", key, value)?;
            }
        }

        Ok(())
    }

    fn write_rule(
        &self,
        _entity: &Entity,
        rule: &EntityRule,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match rule {
            EntityRule::Mono {
                source,
                target: rule,
                r#type,
                rule_source: _,
                metadata,
            } => {
                write!(f, "{} ", source.as_ref())?;
                write!(f, "{} ", r#type.as_ref())?;
                write!(f, "{} ", rule.as_ref())?;
                if let Some(metadata) = metadata {
                    self.write_metadata(metadata, f)?;
                }
                writeln!(f)
            }
            EntityRule::Multi {
                source,
                targets: rules,
                r#type,
                rule_source: _,
                metadata,
            } => {
                write!(f, "{} ", source.as_ref())?;
                write!(f, "{} ", r#type.as_ref())?;
                write!(
                    f,
                    "{} ",
                    rules
                        .iter()
                        .map(|r| r.as_ref())
                        .collect::<Vec<_>>()
                        .join(",")
                )?;
                if let Some(metadata) = metadata {
                    self.write_metadata(metadata, f)?;
                }
                writeln!(f)
            }
        }
    }

    pub fn write_entity(
        &self,
        entity: &Entity,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        for rule in entity.requires.iter() {
            self.write_rule(entity, rule, f)?;
        }

        for rule in entity.excludes.iter() {
            self.write_rule(entity, rule, f)?;
        }

        Ok(())
    }

    fn new(entities: &'a Vec<Entity>) -> Self {
        Self(entities)
    }

    pub fn format(entities: &'a Vec<Entity>) -> String {
        let formatter = Self::new(entities);

        format!("{}", formatter)
    }
}
