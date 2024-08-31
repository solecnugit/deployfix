use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use log::debug;

use crate::{
    model::{
        Entity, EntityName, EntityRule, EntityRuleMetadata, EntityRuleSource,
        EntityRuleTopologyKey, EntityRuleType, METADATA_TOPOLOGY_KEY,
    },
    util,
};

use super::spec::{
    parse_placement_spec_list, CompositeConstraint, Constraint, ConstraintExpr, PlacementSpec,
    PlacementSpecList, SingleConstraint,
};

pub struct YarnSpecParser;

impl YarnSpecParser {
    pub fn new() -> Self {
        YarnSpecParser {}
    }

    fn scope_to_entity_rule_topology_key(scope: &str) -> Option<EntityRuleTopologyKey> {
        match scope {
            "NODE" => Some(EntityRuleTopologyKey::Node),
            "RACK" => Some(EntityRuleTopologyKey::Rack),
            _ => None,
        }
    }

    fn parse_single_constraint(
        &self,
        number: i32,
        constraint: SingleConstraint,
        source: &str,
        idx: usize,
        path: &Path,
    ) -> anyhow::Result<Vec<EntityRule>> {
        let source = EntityName(source.to_string());

        match constraint {
            SingleConstraint::In { scope, target_tag } => {
                let topology = match Self::scope_to_entity_rule_topology_key(scope.as_ref()) {
                    Some(topology) => topology,
                    None => {
                        anyhow::bail!(
                            "Unknown scope: {:?} at {}:{}",
                            scope,
                            path.display(),
                            idx + 1
                        )
                    }
                };

                Ok(vec![EntityRule::mono(
                    source,
                    target_tag.into(),
                    EntityRuleType::Require,
                    EntityRuleSource::File(path.display().to_string(), idx + 1),
                    Some(EntityRuleMetadata::new(
                        path.display().to_string().into(),
                        NonZeroUsize::new(idx + 1),
                        Some(
                            vec![
                                ("scope".to_string(), scope.as_ref().to_string()),
                                ("numberOfContainer".to_string(), number.to_string()),
                                (METADATA_TOPOLOGY_KEY.to_string(), topology.to_string()),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )),
                )])
            }
            SingleConstraint::NotIn { scope, target_tag } => {
                let topology = match Self::scope_to_entity_rule_topology_key(scope.as_ref()) {
                    Some(topology) => topology,
                    None => {
                        anyhow::bail!(
                            "Unknown scope: {:?} at {}:{}",
                            scope,
                            path.display(),
                            idx + 1
                        )
                    }
                };

                Ok(vec![EntityRule::mono(
                    source,
                    target_tag.into(),
                    EntityRuleType::Exclude,
                    EntityRuleSource::File(path.display().to_string(), idx + 1),
                    Some(EntityRuleMetadata::new(
                        path.display().to_string().into(),
                        NonZeroUsize::new(idx + 1),
                        Some(
                            vec![
                                ("scope".to_string(), scope.as_ref().to_string()),
                                ("numberOfContainer".to_string(), number.to_string()),
                                (METADATA_TOPOLOGY_KEY.to_string(), topology.to_string()),
                            ]
                            .into_iter()
                            .collect(),
                        ),
                    )),
                )])
            }
            SingleConstraint::Cardinality {
                scope: _,
                target_tag: _,
                min_card: _,
                max_card: _,
            } => {
                panic!("Cardinality constraint is not supported yet")
            }
        }
    }

    fn parse_composite_constraint(
        &self,
        number: i32,
        constraints: CompositeConstraint,
        source: &str,
        idx: usize,
        path: &Path,
    ) -> anyhow::Result<Vec<EntityRule>> {
        match constraints {
            // The rules are taken conjunctively by default
            CompositeConstraint::And(constraints) => Ok(constraints
                .into_iter()
                .filter_map(|constraint| {
                    match self.parse_constraint(number, constraint, source, idx, path) {
                        Ok(rules) => Some(rules),
                        Err(e) => {
                            debug!(
                                "Failed to parse constraint at {}:{}: {}",
                                path.display(),
                                idx + 1,
                                e
                            );
                            None
                        }
                    }
                })
                .flatten()
                .collect()),
            CompositeConstraint::Or(constraints) => {
                let rules = constraints
                    .into_iter()
                    .filter_map(|constraint| {
                        match self.parse_constraint(number, constraint, source, idx, path) {
                            Ok(rules) => Some(rules),
                            Err(e) => {
                                debug!(
                                    "Failed to parse constraint at {}:{}: {}",
                                    path.display(),
                                    idx + 1,
                                    e
                                );
                                None
                            }
                        }
                    })
                    .flatten()
                    .collect::<Vec<_>>();

                let is_all_require_rule = rules
                    .iter()
                    .all(|rule| rule.r#type() == EntityRuleType::Require);

                let is_all_the_same_scope = rules.iter().all(|rule| {
                    let scope = rule.metadata("scope").unwrap_or("NODE");

                    scope == rules[0].metadata("scope").unwrap_or("NODE")
                });

                let is_all_conflict_rule = rules
                    .iter()
                    .all(|rule| rule.r#type() == EntityRuleType::Exclude);

                if is_all_require_rule && is_all_the_same_scope {
                    let source = EntityName(source.to_string());
                    // Composite OR constraint with all require rules is equivalent to a single require rule
                    return Ok(vec![EntityRule::multi(
                        source,
                        rules
                            .into_iter()
                            .flat_map(|rule| {
                                rule.targets().into_iter().cloned().collect::<Vec<_>>()
                            })
                            .collect(),
                        EntityRuleType::Require,
                        EntityRuleSource::File(path.display().to_string(), idx + 1),
                        Some(EntityRuleMetadata::new(
                            path.display().to_string().into(),
                            NonZeroUsize::new(idx + 1),
                            Some(
                                vec![
                                    ("scope".to_string(), "NODE".to_string()),
                                    ("numberOfContainer".to_string(), number.to_string()),
                                    (
                                        METADATA_TOPOLOGY_KEY.to_string(),
                                        EntityRuleTopologyKey::Node.to_string(),
                                    ),
                                ]
                                .into_iter()
                                .collect(),
                            ),
                        )),
                    )]);
                }

                if is_all_conflict_rule && is_all_the_same_scope {
                    return Ok(rules);
                }

                panic!("Composite OR constraint is only partially supported yet")
            }
        }
    }

    fn parse_constraint(
        &self,
        number: i32,
        constraint: Constraint,
        source: &str,
        idx: usize,
        path: &Path,
    ) -> anyhow::Result<Vec<EntityRule>> {
        match constraint {
            Constraint::Single(constraint) => {
                self.parse_single_constraint(number, constraint, source, idx, path)
            }
            Constraint::Composite(constraint) => {
                self.parse_composite_constraint(number, constraint, source, idx, path)
            }
        }
    }

    fn parse_placement_spec(
        &self,
        spec: PlacementSpec,
        idx: usize,
        path: &Path,
    ) -> Vec<EntityRule> {
        let PlacementSpec {
            source_tag,
            constraint_expr,
        } = spec;

        let source_tag = source_tag.to_string();

        if matches!(constraint_expr, ConstraintExpr::NumContainers(_)) {
            return vec![];
        }

        let (number, constraint) = match constraint_expr {
            ConstraintExpr::NumContainersWithConstraint(number, constraint) => (number, constraint),
            _ => unreachable!(),
        };

        let rules = self.parse_constraint(number, constraint, source_tag.as_ref(), idx, path);

        match rules {
            Ok(rules) => rules,
            Err(e) => {
                debug!(
                    "Failed to parse constraint at {}:{}: {}",
                    path.display(),
                    idx + 1,
                    e
                );
                vec![]
            }
        }
    }

    fn parse_placement_specs(
        &self,
        specs: PlacementSpecList,
        idx: usize,
        path: &Path,
    ) -> Vec<Entity> {
        let rules = specs
            .specs
            .into_iter()
            .flat_map(|spec| self.parse_placement_spec(spec, idx, path))
            .collect();

        util::rule_set_to_entity_set(rules)
    }

    pub fn parse(&self, data: &str, path: PathBuf) -> anyhow::Result<Vec<Entity>> {
        let path = &path;
        let entities = data
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| {
                let line = line.trim();

                if line.is_empty() {
                    return None;
                }

                let (left, specs) = parse_placement_spec_list(line).unwrap();
                assert!(left.is_empty());

                let entities = self.parse_placement_specs(specs, idx, path);

                if entities.is_empty() {
                    return None;
                }

                Some(entities)
            })
            .flatten()
            .collect();

        Ok(entities)
    }
}
