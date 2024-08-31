use std::collections::{BTreeSet, HashMap, HashSet};

use log::{debug, warn};

use crate::model::{Entity, EntityRule, EntityRuleType};

pub fn split_by_metadata(
    entities: &[Entity],
    meta_key: &str,
    default_meta_key: &str,
) -> HashMap<String, Vec<Entity>> {
    entities
        .iter()
        .map(|entity| {
            let requires = &entity.requires;
            let conflicts = &entity.excludes;

            let require_topo = requires.iter().fold(HashMap::new(), |mut acc, rule| {
                let key = rule.metadata(meta_key);

                let key = match key {
                    Some(key) => key,
                    None => {
                        warn!(
                            "Missing `{}` for rule {:?}, assuming the default value {}",
                            meta_key, rule, default_meta_key
                        );

                        default_meta_key
                    }
                };

                let rules: &mut Vec<EntityRule> = acc.entry(key.to_string()).or_default();
                rules.push(rule.clone());

                acc
            });

            let conflict_topo = conflicts.iter().fold(HashMap::new(), |mut acc, rule| {
                let key = rule.metadata(meta_key);

                let key = match key {
                    Some(key) => key,
                    None => {
                        warn!(
                            "Missing `{}` for rule {:?}, assuming the default value {}",
                            meta_key, rule, default_meta_key
                        );

                        default_meta_key
                    }
                };

                let rules: &mut Vec<EntityRule> = acc.entry(key.to_string()).or_default();
                rules.push(rule.clone());

                acc
            });

            let keys = require_topo
                .keys()
                .chain(conflict_topo.keys())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();

            debug!("Topology keys: {:?}", keys);

            let entities = keys
                .into_iter()
                .map(|key| {
                    let requires = require_topo
                        .get(key)
                        .unwrap_or(&Vec::new())
                        .clone()
                        .into_iter()
                        .collect::<BTreeSet<_>>();
                    let conflicts = conflict_topo
                        .get(key)
                        .unwrap_or(&Vec::new())
                        .clone()
                        .into_iter()
                        .collect::<BTreeSet<_>>();

                    (
                        key.to_string(),
                        Entity {
                            name: entity.name.clone(),
                            requires,
                            excludes: conflicts,
                            source: entity.source.clone(),
                            priority: entity.priority.clone(),
                        },
                    )
                })
                .collect::<Vec<_>>();

            entities
        })
        .fold(HashMap::new(), |mut acc, e| {
            for (key, entity) in e {
                let entities = acc.entry(key).or_default();
                entities.push(entity);
            }

            acc
        })
}

pub fn rule_set_to_entity_set(rules: Vec<EntityRule>) -> Vec<Entity> {
    let mut entities = HashMap::new();

    for rule in rules {
        let name = rule.source().as_ref().to_string();
        let entity = entities
            .entry(name.clone())
            .or_insert_with(|| Entity::new(name.as_str()));

        match rule.r#type() {
            EntityRuleType::Require => {
                entity.requires.insert(rule);
            }
            EntityRuleType::Exclude => {
                entity.excludes.insert(rule);
            }
        }
    }

    entities.into_values().collect()
}
