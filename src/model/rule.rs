use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    num::NonZeroUsize,
};

use log::debug;
use serde::{Deserialize, Serialize};

use super::{EntityName, EntityRuleTopologyKey, METADATA_TOPOLOGY_KEY};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EntityRuleSource {
    File(String, usize),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum EntityRuleType {
    Require,
    Exclude,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct EntityRuleMetadata {
    file: Option<String>,
    line: Option<NonZeroUsize>,
    #[serde(flatten)]
    metadata: Option<BTreeMap<String, String>>,
}

impl EntityRuleMetadata {
    pub fn new(
        file: Option<String>,
        line: Option<NonZeroUsize>,
        metadata: Option<BTreeMap<String, String>>,
    ) -> Self {
        Self {
            file,
            line,
            metadata,
        }
    }

    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    pub fn line(&self) -> Option<usize> {
        self.line.map(|l| l.get())
    }

    pub fn topology_key(&self) -> Option<&str> {
        if let Some(metadata) = &self.metadata {
            metadata.get(METADATA_TOPOLOGY_KEY).map(|e| e.as_str())
        } else {
            None
        }
    }

    pub fn get_metadata(&self) -> Option<&BTreeMap<String, String>> {
        self.metadata.as_ref()
    }

    pub fn add_metadata(&mut self, key: String, value: String) {
        if let Some(metadata) = &mut self.metadata {
            if metadata.contains_key(&key) {
                debug!(
                    "Metadata {:?} already exists, and has been replaced by {}={} ",
                    metadata, key, value
                );
            }

            metadata.insert(key, value);
        } else {
            let mut metadata = BTreeMap::new();
            metadata.insert(key, value);
            self.metadata = Some(metadata);
        }
    }
}

impl Display for EntityRuleMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(file) = &self.file {
            write!(f, "{}", file)?;
        }

        if let Some(line) = &self.line {
            write!(f, ":{}", line)?;
        }

        if let Some(metadata) = &self.metadata {
            write!(f, " ")?;
            for (key, value) in metadata.iter() {
                write!(f, "{}={};", key, value)?;
            }
        }

        Ok(())
    }
}

impl Default for EntityRuleSource {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Display for EntityRuleSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityRuleSource::File(path, line) => write!(f, "{}:{}", path, line),
            EntityRuleSource::Unknown => write!(f, "unknown"),
        }
    }
}

impl EntityRuleSource {
    pub fn new(path: &str, line: usize) -> Self {
        Self::File(path.to_string(), line)
    }

    pub fn file(&self) -> Option<&str> {
        match self {
            EntityRuleSource::File(path, _) => Some(path.as_str()),
            EntityRuleSource::Unknown => None,
        }
    }

    pub fn line(&self) -> Option<usize> {
        match self {
            EntityRuleSource::File(_, line) => Some(*line),
            EntityRuleSource::Unknown => None,
        }
    }
}

impl Display for EntityRuleType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityRuleType::Require => write!(f, "require"),
            EntityRuleType::Exclude => write!(f, "exclude"),
        }
    }
}

impl AsRef<str> for EntityRuleType {
    fn as_ref(&self) -> &str {
        match self {
            EntityRuleType::Require => "require",
            EntityRuleType::Exclude => "exclude",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "tag")]
pub enum EntityRule {
    Mono {
        source: EntityName,
        target: EntityName,
        r#type: EntityRuleType,
        #[serde(default = "EntityRuleSource::default")]
        rule_source: EntityRuleSource,
        #[serde(default)]
        metadata: Option<EntityRuleMetadata>,
    },
    Multi {
        source: EntityName,
        targets: BTreeSet<EntityName>,
        r#type: EntityRuleType,
        #[serde(default = "EntityRuleSource::default")]
        rule_source: EntityRuleSource,
        #[serde(default)]
        metadata: Option<EntityRuleMetadata>,
    },
}

impl EntityRule {
    pub fn mono(
        source: EntityName,
        target: EntityName,
        r#type: EntityRuleType,
        rule_source: EntityRuleSource,
        metadata: Option<EntityRuleMetadata>,
    ) -> Self {
        Self::Mono {
            source,
            target,
            r#type,
            rule_source,
            metadata,
        }
    }

    pub fn multi(
        source: EntityName,
        targets: BTreeSet<EntityName>,
        r#type: EntityRuleType,
        rule_source: EntityRuleSource,
        metadata: Option<EntityRuleMetadata>,
    ) -> Self {
        Self::Multi {
            source,
            targets,
            r#type,
            rule_source,
            metadata,
        }
    }

    pub fn source(&self) -> &EntityName {
        match self {
            Self::Mono { source, .. } => source,
            Self::Multi { source, .. } => source,
        }
    }

    pub fn set_rule_source(&mut self, new_source: EntityRuleSource) {
        match self {
            Self::Mono {
                rule_source: source,
                ..
            } => *source = new_source,
            Self::Multi {
                rule_source: source,
                ..
            } => *source = new_source,
        }
    }

    pub fn meta_file(&self) -> Option<&str> {
        match self {
            Self::Mono { metadata, .. } => metadata.as_ref().and_then(|e| e.file.as_deref()),
            Self::Multi { metadata, .. } => metadata.as_ref().and_then(|e| e.file.as_deref()),
        }
    }

    pub fn meta_line(&self) -> Option<usize> {
        match self {
            Self::Mono { metadata, .. } => metadata.as_ref().and_then(|e| e.line.map(usize::from)),
            Self::Multi { metadata, .. } => metadata.as_ref().and_then(|e| e.line.map(usize::from)),
        }
    }

    pub fn meta_topology(&self) -> Option<EntityRuleTopologyKey> {
        match self {
            Self::Mono { metadata, .. } => metadata
                .as_ref()
                .and_then(|e| e.topology_key().map(|e| e.into())),
            Self::Multi { metadata, .. } => metadata
                .as_ref()
                .and_then(|e| e.topology_key().map(|e| e.into())),
        }
    }

    pub fn file(&self) -> Option<&str> {
        match self {
            Self::Mono {
                rule_source: source,
                ..
            } => match source {
                EntityRuleSource::File(path, _) => Some(path.as_str()),
                EntityRuleSource::Unknown => None,
            },
            Self::Multi {
                rule_source: source,
                ..
            } => match source {
                EntityRuleSource::File(path, _) => Some(path.as_str()),
                EntityRuleSource::Unknown => None,
            },
        }
    }

    pub fn line(&self) -> Option<usize> {
        match self {
            Self::Mono {
                rule_source: source,
                ..
            } => match source {
                EntityRuleSource::File(_, line) => Some(*line),
                EntityRuleSource::Unknown => None,
            },
            Self::Multi {
                rule_source: source,
                ..
            } => match source {
                EntityRuleSource::File(_, line) => Some(*line),
                EntityRuleSource::Unknown => None,
            },
        }
    }

    pub fn range(&self) -> Option<(usize, usize)> {
        let start = self.metadata("index").map(|e| e.parse().unwrap_or(0usize));
        let len = self.metadata("len").map(|e| e.parse().unwrap_or(0usize));

        if let (Some(start), Some(len)) = (start, len) {
            Some((start, start + len))
        } else {
            None
        }
    }

    pub fn metadata(&self, key: &str) -> Option<&str> {
        match self {
            Self::Mono { metadata, .. } => metadata
                .as_ref()
                .and_then(|e| e.metadata.as_ref().map(|m| m.get(key).map(|e| e.as_str())))
                .flatten(),
            Self::Multi { metadata, .. } => metadata
                .as_ref()
                .and_then(|e| e.metadata.as_ref().map(|m| m.get(key).map(|e| e.as_str())))
                .flatten(),
        }
    }

    pub fn r#type(&self) -> EntityRuleType {
        match self {
            Self::Mono { r#type, .. } => r#type.clone(),
            Self::Multi { r#type, .. } => r#type.clone(),
        }
    }

    pub fn targets(&self) -> Vec<&EntityName> {
        match self {
            Self::Mono { target, .. } => vec![target],
            Self::Multi { targets, .. } => targets.iter().collect(),
        }
    }

    pub fn is_require(&self) -> bool {
        match self {
            Self::Mono { r#type, .. } => r#type == &EntityRuleType::Require,
            Self::Multi { r#type, .. } => r#type == &EntityRuleType::Require,
        }
    }

    pub fn is_exclude(&self) -> bool {
        match self {
            Self::Mono { r#type, .. } => r#type == &EntityRuleType::Exclude,
            Self::Multi { r#type, .. } => r#type == &EntityRuleType::Exclude,
        }
    }

    pub fn is_multi(&self) -> bool {
        matches!(self, Self::Multi { .. })
    }

    pub fn is_mono(&self) -> bool {
        matches!(self, Self::Mono { .. })
    }

    pub fn is_in_target(&self, target: &str) -> bool {
        match self {
            Self::Mono { target, .. } => target == target,
            Self::Multi { targets, .. } => targets.contains(&EntityName(target.to_string())),
        }
    }
}

impl Display for EntityRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityRule::Mono {
                source: _,
                target,
                r#type,
                rule_source,
                metadata,
            } => {
                write!(f, "[{}] ", r#type.as_ref())?;
                write!(f, "{}", target.as_ref())?;
                if let Some(metadata) = metadata {
                    write!(f, " {}", metadata)?;
                }
                write!(f, " ({})", rule_source)
            }
            EntityRule::Multi {
                source: _,
                targets,
                r#type,
                rule_source,
                metadata,
            } => {
                write!(f, "[{}] ", r#type.as_ref())?;
                write!(
                    f,
                    "{}",
                    targets
                        .iter()
                        .map(|r| r.as_ref())
                        .collect::<Vec<_>>()
                        .join("|")
                )?;
                if let Some(metadata) = metadata {
                    write!(f, " {}", metadata)?;
                }
                write!(f, " ({})", rule_source)
            }
        }
    }
}
