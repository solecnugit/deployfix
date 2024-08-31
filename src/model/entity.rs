use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};

use super::rule::EntityRule;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntityName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EntitySource {
    File(String),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EntityPriority {
    Critical,
    Default,
}

impl Default for EntityPriority {
    fn default() -> Self {
        Self::Default
    }
}

impl From<&str> for EntityPriority {
    fn from(val: &str) -> Self {
        match val {
            "critical" => Self::Critical,
            _ => Self::Default,
        }
    }
}

impl EntityPriority {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Critical => "critical",
            Self::Default => "default",
        }
    }

    pub fn is_critical(&self) -> bool {
        matches!(self, Self::Critical)
    }

    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Entity {
    pub name: EntityName,
    #[serde(default)]
    pub requires: BTreeSet<EntityRule>,
    #[serde(default)]
    pub excludes: BTreeSet<EntityRule>,
    #[serde(default = "EntitySource::default")]
    pub source: EntitySource,
    #[serde(default)]
    pub priority: EntityPriority,
}

pub struct EntityRuleIter<'a> {
    requires: std::collections::btree_set::Iter<'a, EntityRule>,
    excludes: std::collections::btree_set::Iter<'a, EntityRule>,
}

impl<'a> Iterator for EntityRuleIter<'a> {
    type Item = &'a EntityRule;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(r) = self.requires.next() {
            Some(r)
        } else {
            self.excludes.next()
        }
    }
}

impl Entity {
    pub fn new(name: &str) -> Self {
        Self {
            name: EntityName(name.to_string()),
            requires: BTreeSet::new(),
            excludes: BTreeSet::new(),
            source: EntitySource::Unknown,
            priority: EntityPriority::Default,
        }
    }

    pub fn new_with_source(name: &str, source: EntitySource) -> Self {
        Self {
            name: EntityName(name.to_string()),
            requires: BTreeSet::new(),
            excludes: BTreeSet::new(),
            source,
            priority: EntityPriority::Default,
        }
    }

    pub fn new_with_source_and_priority(
        name: &str,
        source: EntitySource,
        priority: EntityPriority,
    ) -> Self {
        Self {
            name: EntityName(name.to_string()),
            requires: BTreeSet::new(),
            excludes: BTreeSet::new(),
            source,
            priority,
        }
    }

    pub fn add_require(&mut self, rule: EntityRule) {
        assert!(rule.is_require(), "rule must be require");

        self.requires.insert(rule);
    }

    pub fn add_exclude(&mut self, rule: EntityRule) {
        assert!(rule.is_exclude(), "rule must be exclude");

        self.excludes.insert(rule);
    }

    pub fn set_source(&mut self, source: EntitySource) {
        self.source = source;
    }

    pub fn rules_len(&self) -> usize {
        self.requires.len() + self.excludes.len()
    }

    pub fn rules(&self) -> EntityRuleIter<'_> {
        EntityRuleIter {
            requires: self.requires.iter(),
            excludes: self.excludes.iter(),
        }
    }

    pub fn is_dummy(&self) -> bool {
        self.rules_len() == 0
    }
}

impl From<&str> for EntityName {
    fn from(name: &str) -> Self {
        Self(name.to_string())
    }
}

impl From<String> for EntityName {
    fn from(name: String) -> Self {
        Self(name)
    }
}

impl AsRef<str> for EntityName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub fn merge_entities(
    entities: Vec<Entity>,
    merge_source: Option<fn(&mut EntitySource, EntitySource)>,
) -> Vec<Entity> {
    let mut map: HashMap<EntityName, Entity> = HashMap::new();

    for entity in entities {
        if let Some(e) = map.get_mut(&entity.name) {
            e.requires.extend(entity.requires);
            e.excludes.extend(entity.excludes);

            if entity.source != e.source {
                if let Some(merge_source) = merge_source {
                    merge_source(&mut e.source, entity.source);
                }
            }
        } else {
            map.insert(entity.name.clone(), entity);
        }
    }

    map.into_values().collect()
}

impl Default for EntitySource {
    fn default() -> Self {
        Self::Unknown
    }
}

impl AsRef<str> for EntitySource {
    fn as_ref(&self) -> &str {
        match self {
            EntitySource::File(path) => path.as_ref(),
            EntitySource::Unknown => "unknown",
        }
    }
}

impl From<PathBuf> for EntitySource {
    fn from(path: PathBuf) -> Self {
        Self::File(path.to_str().unwrap().to_string())
    }
}

impl From<&EntitySource> for String {
    fn from(val: &EntitySource) -> Self {
        match val {
            EntitySource::File(path) => path.clone(),
            EntitySource::Unknown => "unknown".to_string(),
        }
    }
}

impl From<EntitySource> for String {
    fn from(val: EntitySource) -> Self {
        match val {
            EntitySource::File(path) => path,
            EntitySource::Unknown => "unknown".to_string(),
        }
    }
}
