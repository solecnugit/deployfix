use std::collections::{BTreeSet, HashMap, HashSet};

use log::warn;
use thiserror::Error;

use crate::model::{Entity, EntityName, EntityRule};

#[derive(Debug, serde::Serialize)]
pub struct EntityMap {
    pub entities: Vec<Entity>,
    pub names: HashSet<String>,
    pub self_conflicts: HashSet<String>,
}

#[derive(Debug, Error)]
pub enum EntityMapError {
    #[error("Duplicate entity names: {:?}", _0)]
    DuplicateEntityName(Vec<String>),
}

impl EntityMap {
    // Checks for duplicate entity names in the given slice of entities.
    // Returns `Ok(())` if there are no duplicates, otherwise returns `Err` with a list of duplicate names.
    fn check_duplicate_names(entities: &[Entity]) -> Result<(), EntityMapError> {
        let duplicates = entities
            .iter()
            .map(|e| e.name.0.as_str())
            .collect::<Vec<_>>()
            .into_iter()
            .fold(HashMap::new(), |mut acc, name| {
                *acc.entry(name).or_insert(0) += 1;
                acc
            })
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(name, _)| name.to_string())
            .collect::<Vec<_>>();

        match duplicates.len() {
            0 => Ok(()),
            _ => Err(EntityMapError::DuplicateEntityName(duplicates)),
        }
    }

    // Splits the given set of entity rules based on the provided name mapping.
    // If an entity rule's target name is found in the mapping, it is split into two rules with the mapped names.
    // Returns a new set of split entity rules.
    fn rename_set(set: BTreeSet<EntityRule>, from: &str, to: &[&str]) -> BTreeSet<EntityRule> {
        set.into_iter()
            .flat_map(|r| match r {
                EntityRule::Mono {
                    source,
                    target,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    if target.0 == from {
                        to.iter()
                            .map(|n| {
                                EntityRule::mono(
                                    source.clone(),
                                    n.to_string().into(),
                                    r#type.clone(),
                                    rule_source.clone(),
                                    metadata.clone(),
                                )
                            })
                            .collect::<Vec<_>>()
                    } else {
                        vec![EntityRule::mono(
                            source,
                            target,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    }
                }
                EntityRule::Multi {
                    source,
                    targets,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    vec![EntityRule::multi(
                        source,
                        targets
                            .into_iter()
                            .flat_map(|r| {
                                if r.0 == from {
                                    to.iter().map(|n| n.to_string().into()).collect::<Vec<_>>()
                                } else {
                                    vec![r]
                                }
                            })
                            .collect(),
                        r#type,
                        rule_source,
                        metadata,
                    )]
                }
            })
            .collect::<BTreeSet<_>>()
    }

    // Splits the given set of entity rules based on the provided name mapping.
    // If an entity rule's target name is found in the mapping, it is split into multiple rules with the mapped names.
    // Returns a new set of split entity rules.
    fn split_require_rule(
        rules: BTreeSet<EntityRule>,
        name_mapping: &HashMap<String, (String, String)>,
    ) -> BTreeSet<EntityRule> {
        rules
            .into_iter()
            .flat_map(|r| match r {
                EntityRule::Mono {
                    source,
                    target,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    let name = target.0.as_str();
                    if name_mapping.contains_key(name) {
                        let (e1_name, e2_name) = name_mapping.get(name).unwrap();
                        let targets = vec![e1_name.clone().into(), e2_name.clone().into()]
                            .into_iter()
                            .collect();

                        vec![EntityRule::multi(
                            source,
                            targets,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    } else {
                        vec![EntityRule::mono(
                            source,
                            target,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    }
                }
                EntityRule::Multi {
                    source,
                    targets,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    let flag = targets
                        .iter()
                        .any(|r| name_mapping.contains_key(r.0.as_str()));
                    if flag {
                        let targets = targets
                            .into_iter()
                            .flat_map(|r| {
                                let name = r.0.as_str();
                                if name_mapping.contains_key(name) {
                                    let (e1_name, e2_name) = name_mapping.get(name).unwrap();

                                    vec![e1_name.clone().into(), e2_name.clone().into()]
                                } else {
                                    vec![r]
                                }
                            })
                            .collect();

                        vec![
                            EntityRule::multi(
                                source.clone(),
                                targets,
                                r#type.clone(),
                                rule_source.clone(),
                                metadata.clone(),
                            ),
                            // EntityRule::multi(source, t2, r#type, rule_source, metadata),
                        ]
                    } else {
                        vec![EntityRule::multi(
                            source,
                            targets,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    }
                }
            })
            .collect::<BTreeSet<_>>()
    }

    fn split_exclude_rules(
        rules: BTreeSet<EntityRule>,
        name_mapping: &HashMap<String, (String, String)>,
    ) -> BTreeSet<EntityRule> {
        rules
            .into_iter()
            .flat_map(|r| match r {
                EntityRule::Mono {
                    source,
                    target,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    let name = target.0.as_str();
                    if name_mapping.contains_key(name) {
                        let (e1_name, e2_name) = name_mapping.get(name).unwrap();

                        vec![
                            EntityRule::mono(
                                source.clone(),
                                e1_name.clone().into(),
                                r#type.clone(),
                                rule_source.clone(),
                                metadata.clone(),
                            ),
                            EntityRule::mono(
                                source,
                                e2_name.clone().into(),
                                r#type,
                                rule_source,
                                metadata,
                            ),
                        ]
                    } else {
                        vec![EntityRule::mono(
                            source,
                            target,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    }
                }
                EntityRule::Multi {
                    source,
                    targets,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    let flag = targets
                        .iter()
                        .any(|r| name_mapping.contains_key(r.0.as_str()));
                    if flag {
                        let t1_targets = targets
                            .iter()
                            .map(|r| {
                                let name = r.0.as_str();
                                if name_mapping.contains_key(name) {
                                    let (e1_name, _) = name_mapping.get(name).unwrap();
                                    e1_name.to_string().into()
                                } else {
                                    r.clone()
                                }
                            })
                            .collect::<BTreeSet<_>>();

                        let t2_targets = targets
                            .iter()
                            .map(|r| {
                                let name = r.0.as_str();
                                if name_mapping.contains_key(name) {
                                    let (_, e2_name) = name_mapping.get(name).unwrap();
                                    e2_name.to_string().into()
                                } else {
                                    r.clone()
                                }
                            })
                            .collect::<BTreeSet<_>>();

                        vec![
                            EntityRule::multi(
                                source.clone(),
                                t1_targets,
                                r#type.clone(),
                                rule_source.clone(),
                                metadata.clone(),
                            ),
                            EntityRule::multi(source, t2_targets, r#type, rule_source, metadata),
                        ]
                    } else {
                        vec![EntityRule::multi(
                            source,
                            targets,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    }
                }
            })
            .collect::<BTreeSet<_>>()
    }

    fn force_split_rule(
        rules: BTreeSet<EntityRule>,
        from: &str,
        to: &[&str],
    ) -> BTreeSet<EntityRule> {
        rules
            .into_iter()
            .map(|rule| match rule {
                EntityRule::Mono {
                    source,
                    target,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    if target.0 == from {
                        to.iter()
                            .map(|e| {
                                EntityRule::mono(
                                    source.clone(),
                                    e.to_string().into(),
                                    r#type.clone(),
                                    rule_source.clone(),
                                    metadata.clone(),
                                )
                            })
                            .collect::<Vec<_>>()
                    } else {
                        vec![EntityRule::mono(
                            source,
                            target,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    }
                }
                EntityRule::Multi {
                    source,
                    targets,
                    r#type,
                    rule_source,
                    metadata,
                } => {
                    if targets.iter().any(|r| r.0 == from) {
                        to.iter()
                            .map(|e| {
                                EntityRule::multi(
                                    source.clone(),
                                    targets
                                        .iter()
                                        .map(|r| {
                                            if r.0 == from {
                                                e.to_string().into()
                                            } else {
                                                r.clone()
                                            }
                                        })
                                        .collect::<BTreeSet<_>>(),
                                    r#type.clone(),
                                    rule_source.clone(),
                                    metadata.clone(),
                                )
                            })
                            .collect::<Vec<_>>()
                    } else {
                        vec![EntityRule::multi(
                            source,
                            targets,
                            r#type,
                            rule_source,
                            metadata,
                        )]
                    }
                }
            })
            .flatten()
            .collect::<BTreeSet<_>>()
    }

    fn preprocessing_self_conflicts(entities: Vec<Entity>) -> (Vec<Entity>, HashSet<String>) {
        let mut name_mapping = HashMap::new();
        let mut self_conflicts = HashSet::new();

        let entities = entities
            .into_iter()
            .flat_map(|e| {
                let name = e.name.0.clone();

                let self_conflict_flag = e.excludes.iter().any(|c| match c {
                    EntityRule::Mono { target: rule, .. } => rule.0.as_str() == name,
                    EntityRule::Multi { targets: rules, .. } => {
                        rules.iter().any(|r| r.0.as_str() == name)
                    }
                });

                if !self_conflict_flag {
                    return vec![e];
                }

                let self_require_flag = e.requires.iter().any(|r| match r {
                    EntityRule::Mono { target: rule, .. } => rule.0.as_str() == name,
                    EntityRule::Multi { targets: rules, .. } => {
                        rules.iter().all(|r| r.0.as_str() == name)
                    }
                });

                if self_require_flag {
                    self_conflicts.insert(name.clone());
                    warn!(
                        "Entity `{}` has both self-affinity and self-anti-affinity",
                        name
                    );
                }

                // Split entity into two entities with suffixes of _1 and _2
                let e1_name = format!("{}_1", name);
                let e2_name = format!("{}_2", name);

                name_mapping.insert(name.clone(), (e1_name.clone(), e2_name.clone()));

                let (mut e1, mut e2) = (e.clone(), e.clone());
                // e1.requires = Self::rename_set(
                //     e1.requires,
                //     name.as_str(),
                //     &[e1_name.as_str(), e2_name.as_str()],
                // );
                e1.requires = Self::force_split_rule(
                    e1.requires,
                    name.as_str(),
                    &[e1_name.as_str(), e2_name.as_str()],
                );
                e1.excludes = Self::rename_set(e1.excludes, name.as_str(), &[e2_name.as_str()]);
                // e1.excludes = Self::split_exclude_rules(e1.excludes, &name_mapping);

                // e2.requires = Self::rename_set(
                //     e2.requires,
                //     name.as_str(),
                //     &[e1_name.as_str(), e2_name.as_str()],
                // );
                e2.requires = Self::force_split_rule(
                    e2.requires,
                    name.as_str(),
                    &[e1_name.as_str(), e2_name.as_str()],
                );
                e2.excludes = Self::rename_set(e2.excludes, name.as_str(), &[e1_name.as_str()]);
                // e2.excludes = Self::split_exclude_rules(e2.excludes, &name_mapping);

                e1.name = e1_name.into();
                e2.name = e2_name.into();

                vec![e1, e2]
            })
            .collect::<Vec<_>>();

        // Rename all entities
        let entities = entities
            .into_iter()
            .map(|mut e| {
                e.requires = Self::split_require_rule(e.requires, &name_mapping);
                e.excludes = Self::split_exclude_rules(e.excludes, &name_mapping);
                e
            })
            .collect::<Vec<_>>();

        (entities, self_conflicts)
    }

    fn collect_entity_names(entities: &[Entity]) -> HashSet<String> {
        entities
            .iter()
            .flat_map(|e| {
                let requires = e
                    .requires
                    .iter()
                    .flat_map(|r| match r {
                        EntityRule::Mono { target: rule, .. } => vec![rule.0.clone()],
                        EntityRule::Multi { targets: rules, .. } => {
                            rules.iter().map(|r| r.0.clone()).collect::<Vec<_>>()
                        }
                    })
                    .collect::<Vec<_>>();

                let conflicts = e
                    .excludes
                    .iter()
                    .flat_map(|r| match r {
                        EntityRule::Mono { target: rule, .. } => vec![rule.0.clone()],
                        EntityRule::Multi { targets: rules, .. } => {
                            rules.iter().map(|r| r.0.clone()).collect::<Vec<_>>()
                        }
                    })
                    .collect::<Vec<_>>();

                let mut names = vec![e.name.0.clone()];
                names.extend(requires);
                names.extend(conflicts);

                names
            })
            .collect::<HashSet<_>>()
    }

    pub fn build(entities: &[Entity]) -> Result<Self, EntityMapError> {
        // Check for duplicate names
        Self::check_duplicate_names(entities)?;

        let (entities, self_conflicts) = Self::preprocessing_self_conflicts(entities.to_owned());
        let names = Self::collect_entity_names(&entities);

        Ok(Self {
            entities,
            names,
            self_conflicts,
        })
    }
}

impl TryFrom<Vec<Entity>> for EntityMap {
    type Error = EntityMapError;

    fn try_from(entities: Vec<Entity>) -> Result<Self, Self::Error> {
        Self::build(&entities)
    }
}

impl TryFrom<&Vec<Entity>> for EntityMap {
    type Error = EntityMapError;

    fn try_from(entities: &Vec<Entity>) -> Result<Self, Self::Error> {
        Self::build(entities)
    }
}
