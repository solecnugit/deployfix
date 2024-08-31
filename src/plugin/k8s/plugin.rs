use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use crate::model::{
    Entity, EntityName, EntityPriority, EntityRule, EntityRuleMetadata, EntityRuleSource,
    EntityRuleTopologyKey, EntityRuleType, EntitySource, METADATA_TOPOLOGY_KEY,
};
use anyhow::Context;
use k8s_openapi::{
    api::{
        apps::v1::Deployment,
        core::v1::{
            Node, NodeAffinity, NodeSelectorRequirement, NodeSelectorTerm, Pod, PodAffinity,
            PodAffinityTerm, PodAntiAffinity, PodSpec,
        },
    },
    apimachinery::pkg::apis::meta::v1::{LabelSelector, LabelSelectorRequirement},
};
use log::{debug, warn};

use serde_yaml::Spanned;

pub const METADATA_RESOURCE_TYPE_KEY: &str = "resource_type";

pub struct K8sPlugin {}

#[derive(Debug, Copy, Clone)]
pub enum ResourceType {
    Pod,
    Deployment,
    Node,
}

impl AsRef<str> for ResourceType {
    fn as_ref(&self) -> &str {
        match self {
            Self::Pod => "pod",
            Self::Deployment => "deployment",
            Self::Node => "node",
        }
    }
}

impl TryFrom<&str> for ResourceType {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "pod" => Ok(Self::Pod),
            "deployment" => Ok(Self::Deployment),
            _ => Err("unknown resource type"),
        }
    }
}

impl K8sPlugin {
    pub fn extract_entity_from_path(path: &Path) -> anyhow::Result<Vec<Entity>> {
        let data = std::fs::read_to_string(path)?;

        let (name, spec, resource_type) =
            if let Ok(deployment) = serde_yaml::from_str::<Deployment>(&data) {
                let spec = deployment.spec.context("missing spec in deployment")?;

                let template = spec.template;
                let metadata = template
                    .metadata
                    .context("missing metadata in deployment.template")?;

                let name = if let Some(name) = deployment.metadata.name {
                    name
                } else if let Some(name) = metadata.name {
                    name
                } else {
                    anyhow::bail!(
                        "missing name in deployment.metadata or deployment.spec.template.metadata"
                    )
                };

                let spec = template
                    .spec
                    .context("missing spec in deployment.template")?;

                (name, spec, ResourceType::Deployment)
            } else if let Ok(pod) = serde_yaml::from_str::<Pod>(&data) {
                let metadata = pod.metadata;

                let name = metadata.name.context("missing name in pod.metadata")?;
                let spec = pod.spec.context("missing spec in pod")?;

                (name, spec, ResourceType::Pod)
            } else if let Ok(node) = serde_yaml::from_str::<Node>(&data) {
                let metadata = node.metadata;
                let labels = metadata.labels;

                if let Some(labels) = labels {
                    let map = labels.value.into_iter().collect();
                    return Self::extract_entity_from_node(&map, path);
                } else {
                    return Ok(vec![]);
                }
            } else {
                anyhow::bail!("Invalid configuration {}", path.display())
            };

        Self::extract_entity(&name, &spec, resource_type, path)
            .context("failed to extract entity")
            .map(|e| vec![e])
    }

    fn topology_key_to_entity_rule_topology_key(
        topology_key: &str,
    ) -> Option<EntityRuleTopologyKey> {
        match topology_key {
            "kubernetes.io/hostname" => Some(EntityRuleTopologyKey::Node),
            "topology.kubernetes.io/hostname" => Some(EntityRuleTopologyKey::Node),
            "topology.kubernetes.io/zone" => Some(EntityRuleTopologyKey::Zone),
            "topology.kubernetes.io/region" => Some(EntityRuleTopologyKey::Zone),
            _ => None,
        }
    }

    fn extract_node_affinity_rules(
        node_affinity: &NodeAffinity,
        entity: &mut Entity,
        resource_type: ResourceType,
        source: &Path,
    ) -> anyhow::Result<()> {
        let terms = node_affinity
            .required_during_scheduling_ignored_during_execution
            .as_ref();

        if terms.is_none() {
            return Ok(());
        }

        let terms = terms.unwrap();
        let terms = &terms.node_selector_terms;

        if terms.is_empty() {
            return Ok(());
        }

        for span in terms {
            let term = &span.value;
            let line = span.line;

            let match_expressions = term
                .match_expressions
                .as_ref()
                .context("Invalid match expressions")?;

            let metadata = EntityRuleMetadata::new(
                Some(source.display().to_string()),
                NonZeroUsize::new(line),
                Some(
                    vec![(
                        METADATA_RESOURCE_TYPE_KEY.to_string(),
                        resource_type.as_ref().to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
            );

            for expr in match_expressions.iter() {
                let key: &str = expr.key.as_ref();
                let operator: &str = expr.operator.as_ref();
                let values: Vec<&str> = expr
                    .values
                    .as_deref()
                    .context("Invalid expression values")?
                    .iter()
                    .map(|s| s.as_ref())
                    .collect();

                let entity_rule_source = EntityRuleSource::File(source.display().to_string(), line);
                let mut metadata = metadata.clone();
                metadata.add_metadata("key".into(), key.into());
                metadata.add_metadata("type".into(), "nodeAffinity".into());
                metadata.add_metadata("topology_key".into(), "kubernetes.io/hostname".into());
                metadata.add_metadata("topology".into(), "node".into());

                match operator {
                    "In" => {
                        metadata.add_metadata("operator".into(), operator.into());
                    }
                    "NotIn" => {
                        warn!("Operator `NotIn` for affinity rule will be transformed into `In` for anti-affinity rule {:?}", expr);
                        warn!("It will be separated into two rules that both are required to be satisfied, which might not be intentional.");
                        metadata.add_metadata("inverse".into(), "true".into());
                        metadata.add_metadata("operator".into(), "In".into());
                    }
                    _ => {
                        panic!("Operator is not support yet: {}", operator)
                    }
                }

                match values.len() {
                    0 => {}
                    1 => {
                        let source = entity.name.clone();
                        let target = format!("{}={}", key, values[0]);

                        match operator {
                            "In" => entity.add_require(EntityRule::mono(
                                source,
                                target.into(),
                                EntityRuleType::Require,
                                entity_rule_source,
                                Some(metadata),
                            )),
                            "NotIn" => entity.add_exclude(EntityRule::mono(
                                source,
                                target.into(),
                                EntityRuleType::Exclude,
                                entity_rule_source,
                                Some(metadata),
                            )),
                            _ => unreachable!(),
                        }
                    }
                    _ => {
                        let source = entity.name.clone();
                        let targets = values
                            .iter()
                            .map(|v| EntityName(format!("{}={}", key, v)))
                            .collect::<BTreeSet<_>>();

                        match operator {
                            "In" => entity.add_require(EntityRule::multi(
                                source,
                                targets,
                                crate::model::EntityRuleType::Require,
                                entity_rule_source,
                                Some(metadata.clone()),
                            )),
                            "NotIn" => entity.add_exclude(EntityRule::multi(
                                source,
                                targets,
                                crate::model::EntityRuleType::Exclude,
                                entity_rule_source,
                                Some(metadata.clone()),
                            )),
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn extract_pod_affinity_rules(
        pod_affinity: &PodAffinity,
        entity: &mut Entity,
        resource_type: ResourceType,
        source: &Path,
    ) -> anyhow::Result<()> {
        let terms = pod_affinity
            .required_during_scheduling_ignored_during_execution
            .as_ref();

        if terms.is_none() {
            return Ok(());
        }

        let terms = terms.unwrap();

        if terms.is_empty() {
            return Ok(());
        }

        for span in terms.iter() {
            let term = &span.value;
            let line = span.line;

            let topology_key: &str = term.topology_key.as_ref();
            let topo = Self::topology_key_to_entity_rule_topology_key(topology_key)
                .context("Invalid topology key")?;
            let label_selector = term
                .label_selector
                .as_ref()
                .context("Invalid label selector")?;
            let match_expressions = label_selector
                .match_expressions
                .as_ref()
                .context("Invalid match expressions")?;

            let metadata = EntityRuleMetadata::new(
                Some(source.display().to_string()),
                NonZeroUsize::new(line),
                Some(
                    vec![
                        ("topology_key".to_string(), topology_key.to_string()),
                        (METADATA_TOPOLOGY_KEY.to_string(), topo.to_string()),
                        (
                            METADATA_RESOURCE_TYPE_KEY.to_string(),
                            resource_type.as_ref().to_string(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            );

            for expr in match_expressions.iter() {
                let key: &str = expr.key.as_ref();
                let operator: &str = expr.operator.as_ref();
                let values: Vec<&str> = expr
                    .values
                    .as_deref()
                    .context("Invalid expression values")?
                    .iter()
                    .map(|s| s.as_ref())
                    .collect();

                let entity_rule_source = EntityRuleSource::File(source.display().to_string(), line);
                let mut metadata = metadata.clone();
                metadata.add_metadata("key".into(), key.into());
                metadata.add_metadata("type".into(), "podAffinity".into());

                match operator {
                    "In" => {
                        metadata.add_metadata("operator".into(), operator.into());
                    }
                    "NotIn" => {
                        warn!("Operator `NotIn` for affinity rule will be transformed into `In` for anti-affinity rule {:?}", expr);
                        warn!("It will be separated into two rules that both are required to be satisfied,which might not be intentional.");
                        metadata.add_metadata("inverse".into(), "true".into());
                        metadata.add_metadata("operator".into(), "In".into());
                    }
                    _ => {
                        panic!("Operator is not support yet: {}", operator)
                    }
                }

                match values.len() {
                    0 => {}
                    1 => {
                        let source = entity.name.clone();
                        let target = format!("{}={}", key, values[0]);

                        match operator {
                            "In" => entity.add_require(EntityRule::mono(
                                source,
                                target.into(),
                                EntityRuleType::Require,
                                entity_rule_source,
                                Some(metadata),
                            )),
                            "NotIn" => entity.add_exclude(EntityRule::mono(
                                source,
                                target.into(),
                                EntityRuleType::Exclude,
                                entity_rule_source,
                                Some(metadata),
                            )),
                            _ => unreachable!(),
                        }
                    }
                    _ => {
                        let source = entity.name.clone();
                        let targets = values
                            .iter()
                            .map(|v| EntityName(format!("{}={}", key, v)))
                            .collect::<BTreeSet<_>>();

                        match operator {
                            "In" => entity.add_require(EntityRule::multi(
                                source,
                                targets,
                                crate::model::EntityRuleType::Require,
                                entity_rule_source,
                                Some(metadata.clone()),
                            )),
                            "NotIn" => entity.add_exclude(EntityRule::multi(
                                source,
                                targets,
                                crate::model::EntityRuleType::Exclude,
                                entity_rule_source,
                                Some(metadata.clone()),
                            )),
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn parse_pod_anti_affinity_rules(
        pod_anti_affinity: &PodAntiAffinity,
        entity: &mut Entity,
        resource_type: ResourceType,
        source: &Path,
    ) -> anyhow::Result<()> {
        let terms = pod_anti_affinity
            .required_during_scheduling_ignored_during_execution
            .as_ref();

        if terms.is_none() {
            return Ok(());
        }

        let terms = terms.unwrap();

        if terms.is_empty() {
            return Ok(());
        }

        for span in terms.iter() {
            let term = &span.value;
            let line = span.line;

            let topology_key: &str = term.topology_key.as_ref();
            let topo = Self::topology_key_to_entity_rule_topology_key(topology_key)
                .context("Invalid topology key")?;
            let label_selector = term
                .label_selector
                .as_ref()
                .context("Invalid label selector")?;
            let match_expressions = label_selector
                .match_expressions
                .as_ref()
                .context("Invalid match expressions")?;

            let metadata = EntityRuleMetadata::new(
                Some(source.display().to_string()),
                NonZeroUsize::new(line),
                Some(
                    vec![
                        ("topology_key".to_string(), topology_key.to_string()),
                        (METADATA_TOPOLOGY_KEY.to_string(), topo.to_string()),
                        (
                            METADATA_RESOURCE_TYPE_KEY.to_string(),
                            resource_type.as_ref().to_string(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                ),
            );

            for expr in match_expressions.iter() {
                let key: &str = expr.key.as_ref();
                let operator: &str = expr.operator.as_ref();
                let values: Vec<&str> = expr
                    .values
                    .as_deref()
                    .context("Invalid expression values")?
                    .iter()
                    .map(|s| s.as_ref())
                    .collect();

                let entity_rule_source = EntityRuleSource::File(source.display().to_string(), line);
                let mut metadata = metadata.clone();
                metadata.add_metadata("key".into(), key.into());
                metadata.add_metadata("type".into(), "podAntiAffinity".into());

                match operator {
                    "In" => {
                        metadata.add_metadata("operator".into(), operator.into());
                    }
                    "NotIn" => {
                        warn!("Operator `NotIn` for anti-affinity rule will be transformed into `In` for affinity rule {:?}", expr);
                        warn!("It will be separated into two rules that both are required to be satisfied, which might not be intentional.");
                        metadata.add_metadata("inverse".into(), "true".into());
                        metadata.add_metadata("operator".into(), "In".into());
                    }
                    _ => {
                        panic!("Operator is not support yet: {}", operator)
                    }
                }

                match values.len() {
                    0 => {}
                    1 => {
                        let source = entity.name.clone();
                        let target = format!("{}={}", key, values[0]);

                        match operator {
                            "In" => entity.add_exclude(EntityRule::mono(
                                source,
                                target.into(),
                                EntityRuleType::Exclude,
                                entity_rule_source,
                                Some(metadata),
                            )),
                            "NotIn" => entity.add_require(EntityRule::mono(
                                source,
                                target.into(),
                                EntityRuleType::Require,
                                entity_rule_source,
                                Some(metadata),
                            )),
                            _ => unreachable!(),
                        }
                    }
                    _ => {
                        let source = entity.name.clone();

                        let targets = values
                            .iter()
                            .map(|v| EntityName(format!("{}={}", key, v)))
                            .collect::<BTreeSet<_>>();

                        match operator {
                            "In" => entity.add_exclude(EntityRule::multi(
                                source,
                                targets,
                                crate::model::EntityRuleType::Exclude,
                                entity_rule_source,
                                Some(metadata.clone()),
                            )),
                            "NotIn" => entity.add_require(EntityRule::multi(
                                source,
                                targets,
                                crate::model::EntityRuleType::Require,
                                entity_rule_source,
                                Some(metadata.clone()),
                            )),
                            _ => unreachable!(),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn extract_entity_from_node(
        labels: &BTreeMap<String, String>,
        path: &Path,
    ) -> anyhow::Result<Vec<Entity>> {
        let name = labels
            .get("kubernetes.io/hostname")
            .expect("No hostname found");

        let entity_source = EntitySource::File(path.display().to_string());
        let entities = labels
            .iter()
            .map(|(key, value)| {
                let entity_name = format!("{}={}", key, value);
                let mut entity = Entity::new_with_source(&entity_name, entity_source.clone());
                entity.priority = EntityPriority::Default;

                entity
            })
            .collect::<Vec<_>>();

        Ok(entities)
    }

    fn extract_entity(
        name: &str,
        pod: &PodSpec,
        resource_type: ResourceType,
        source: &Path,
    ) -> anyhow::Result<Entity> {
        // FIXME: This is a assumption that all labels are app=xxx
        let name = format!("app={}", name);

        let entity_source = EntitySource::File(source.display().to_string());
        let mut entity = Entity::new_with_source(&name, entity_source);

        entity.priority = pod
            .priority_class_name
            .as_ref()
            .map(|e| EntityPriority::from(e.as_str()))
            .unwrap_or_default();

        let affinity = pod.affinity.as_ref();
        let affinity = match affinity {
            Some(affinity) => affinity,
            None => return Ok(entity),
        };

        let node_affinity = affinity.node_affinity.as_ref();
        if let Some(node_affinity) = node_affinity {
            Self::extract_node_affinity_rules(node_affinity, &mut entity, resource_type, source)?;
        }

        // PodAffinity
        let pod_affinity = affinity.pod_affinity.as_ref();
        if let Some(pod_affinity) = pod_affinity {
            Self::extract_pod_affinity_rules(pod_affinity, &mut entity, resource_type, source)?;
        }
        // PodAntiAffinity
        let pod_anti_affinity = affinity.pod_anti_affinity.as_ref();
        if let Some(pod_anti_affinity) = pod_anti_affinity {
            Self::parse_pod_anti_affinity_rules(
                pod_anti_affinity,
                &mut entity,
                resource_type,
                source,
            )?;
        }

        Ok(entity)
    }

    pub fn scan_entity_file_mapping(
        entities: &[Entity],
    ) -> anyhow::Result<HashMap<String, PathBuf>> {
        let mapping = entities
            .iter()
            .flat_map(|entity| {
                let name = entity.name.as_ref();
                let requires = &entity.requires;
                let conflicts = &entity.excludes;
                let entity_source = &entity.source;

                let entity_source_file = if let EntitySource::File(path) = entity_source {
                    vec![(name, path.as_str())]
                } else {
                    vec![]
                };

                requires
                    .iter()
                    .chain(conflicts.iter())
                    .filter_map(|rule| rule.meta_file().map(|e| (name, e)))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .chain(entity_source_file)
            })
            .filter(|(_, path)| !path.ends_with(".ir"))
            .collect::<Vec<_>>();

        // Check is there duplicates
        let duplicates = mapping
            .iter()
            .fold(HashMap::new(), |mut acc, (name, path)| {
                let entry: &mut HashSet<&str> = acc.entry(name).or_default();

                entry.insert(path);

                acc
            })
            .into_iter()
            .filter(|(_, paths)| paths.len() > 1)
            .collect::<Vec<_>>();

        if !duplicates.is_empty() {
            return Err(anyhow::anyhow!(
                "Duplicate entity name with different source: {:?}",
                duplicates
            ));
        }

        Ok(mapping
            .into_iter()
            .map(|(name, path)| (name.into(), path.to_string().into()))
            .collect())
    }

    fn inject_pod_affinity_rules(
        terms: &mut Vec<Spanned<PodAffinityTerm>>,
        rules: &BTreeSet<EntityRule>,
    ) -> anyhow::Result<()> {
        // First Implementation: Clear all existing terms And replace with new terms
        terms.clear();

        for rule in rules.iter() {
            let r#type = rule
                .metadata("type")
                .context("No `type` found in metadata")?;

            match r#type {
                "podAffinity" => {}
                "podAntiAffinity" => {}
                _ => continue,
            }

            let topology_key = rule.metadata("topology_key");

            let topology_key = match topology_key {
                Some(topology_key) => topology_key,
                None => {
                    warn!("No `topology_key` found in metadata for rule {:?}, assuming the default value `topology.kubernetes.io/hostname`", rule);
                    "topology.kubernetes.io/hostname"
                }
            };

            let key = rule.metadata("key");
            let key = match key {
                Some(key) => key,
                None => {
                    warn!("No `key` found in metadata for rule {:?}, assuming the default value `app`", rule);
                    "app"
                }
            };

            let operator = rule.metadata("operator");
            let operator = match operator {
                Some(operator) => operator,
                None => {
                    warn!("No `operator` found in metadata for rule {:?}, assuming the default value `In`", rule);
                    "In"
                }
            };
            let operator = match operator {
                "In" => "In",
                "NotIn" => {
                    warn!("Operator `NotIn` for anti-affinity rule will be transformed into `In` {:?}", rule);
                    warn!("It will be separated into two rules that both are required to be satisfied, which might not be intentional.");
                    "In"
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Invalid operator: {} for rule {:?}",
                        operator,
                        rule
                    ))
                }
            };

            let values = match rule {
                EntityRule::Mono { target: rule, .. } => vec![rule.as_ref()],
                EntityRule::Multi { targets: rules, .. } => {
                    rules.iter().map(|n| n.as_ref()).collect()
                }
            };

            let values = values
                .into_iter()
                .map(|value| {
                    // app=S1 => S1
                    if value.contains('=') {
                        let values = value.split('=').collect::<Vec<_>>();

                        if values.len() != 2 {
                            return Err(anyhow::anyhow!(
                                "Invalid value format: {} for rule {:?}",
                                value,
                                rule
                            ));
                        }

                        let prefix = values[0];

                        if prefix != key {
                            return Err(anyhow::anyhow!(
                                "Invalid value format: {} for rule {:?}",
                                value,
                                rule
                            ));
                        }

                        Ok(values[1].to_string())
                    } else {
                        Ok(value.to_string())
                    }
                })
                .collect::<Vec<_>>();

            let values = values.into_iter().collect::<Result<Vec<_>, _>>()?;

            let term = PodAffinityTerm {
                topology_key: topology_key.into(),
                label_selector: Some(LabelSelector {
                    match_expressions: Some(vec![LabelSelectorRequirement {
                        key: key.into(),
                        operator: operator.into(),
                        values: Some(values),
                    }]),
                    ..Default::default()
                }),
                ..Default::default()
            };

            terms.push(Spanned {
                value: term,
                index: 0,
                line: 0,
                column: 0,
                len: 0,
            });
        }

        Ok(())
    }

    fn inject_node_affinity_rules(
        terms: &mut Vec<Spanned<NodeSelectorTerm>>,
        requires: &BTreeSet<EntityRule>,
        excludes: &BTreeSet<EntityRule>,
    ) -> anyhow::Result<()> {
        // First Implementation: Clear all existing terms And replace with new terms
        terms.clear();

        let mut rules = requires.iter().collect::<Vec<_>>();
        rules.extend(excludes.iter());

        for rule in rules.iter() {
            let r#type = rule
                .metadata("type")
                .context("No `type` found in metadata")?;

            match r#type {
                "nodeAffinity" => {}
                _ => continue,
            }

            let topology_key = rule.metadata("topology_key");

            let topology_key = match topology_key {
                Some(topology_key) => topology_key,
                None => {
                    warn!("No `topology_key` found in metadata for rule {:?}, assuming the default value `kubernetes.io/hostname`", rule);
                    "kubernetes.io/hostname"
                }
            };

            let key = rule.metadata("key");
            let key = match key {
                Some(key) => key,
                None => {
                    warn!("No `key` found in metadata for rule {:?}, assuming the default value `app`", rule);
                    "app"
                }
            };

            let operator = rule.metadata("operator");
            let operator = match operator {
                Some(operator) => operator,
                None => {
                    warn!("No `operator` found in metadata for rule {:?}, assuming the default value `In`", rule);
                    "In"
                }
            };

            let values = match rule {
                EntityRule::Mono { target: rule, .. } => vec![rule.as_ref()],
                EntityRule::Multi { targets: rules, .. } => {
                    rules.iter().map(|n| n.as_ref()).collect()
                }
            };

            let values = values
                .into_iter()
                .map(|value| {
                    // app=S1 => S1
                    if value.contains('=') {
                        let values = value.split('=').collect::<Vec<_>>();

                        if values.len() != 2 {
                            return Err(anyhow::anyhow!(
                                "Invalid value format: {} for rule {:?}",
                                value,
                                rule
                            ));
                        }

                        let prefix = values[0];

                        if prefix != key {
                            return Err(anyhow::anyhow!(
                                "Invalid value format: {} for rule {:?}",
                                value,
                                rule
                            ));
                        }

                        Ok(values[1].to_string())
                    } else {
                        Ok(value.to_string())
                    }
                })
                .collect::<Vec<_>>();

            let values = values.into_iter().collect::<Result<Vec<_>, _>>()?;

            let term = NodeSelectorTerm {
                match_expressions: Some(vec![NodeSelectorRequirement {
                    key: key.into(),
                    operator: operator.into(),
                    values: Some(values),
                }]),
                ..Default::default()
            };

            terms.push(Spanned {
                value: term,
                index: 0,
                line: 0,
                column: 0,
                len: 0,
            });
        }

        Ok(())
    }

    fn inject_entity_to_pod_spec(
        entity: Entity,
        pod_spec: &mut PodSpec,
        // base_name: String,
    ) -> anyhow::Result<()> {
        // let name = entity.name.as_ref();

        let affinity = pod_spec.affinity.get_or_insert(Default::default());

        if !entity.requires.is_empty() {
            let pod_affinity = affinity.pod_affinity.get_or_insert(Default::default());

            let terms = pod_affinity
                .required_during_scheduling_ignored_during_execution
                .get_or_insert(Default::default());

            Self::inject_pod_affinity_rules(terms, &entity.requires)?;
        }

        if !entity.excludes.is_empty() {
            let pod_anti_affinity = affinity.pod_anti_affinity.get_or_insert(Default::default());

            let terms = pod_anti_affinity
                .required_during_scheduling_ignored_during_execution
                .get_or_insert(Default::default());

            Self::inject_pod_affinity_rules(terms, &entity.excludes)?;
        }

        if !entity.requires.is_empty() || !entity.excludes.is_empty() {
            let node_affinity = affinity.node_affinity.get_or_insert(Default::default());

            let terms = node_affinity
                .required_during_scheduling_ignored_during_execution
                .get_or_insert(Default::default());

            let terms = &mut terms.node_selector_terms;

            Self::inject_node_affinity_rules(terms, &entity.requires, &entity.excludes)?;
        }

        Ok(())
    }

    fn inject_entity(entity: Entity, path: &Path) -> anyhow::Result<(String, String)> {
        let _name = entity.name.as_ref();

        let base_name = path.file_name().context("No file name found")?;
        let base_name = base_name.to_str().context("Invalid file name")?;
        let base_name = base_name.to_string();

        let data = std::fs::read_to_string(path)?;

        if let Ok(mut deployment) = serde_yaml::from_str::<Deployment>(&data) {
            let pod_spec = deployment
                .spec
                .as_mut()
                .context("missing spec in deployment")?
                .template
                .spec
                .as_mut()
                .context("missing spec in deployment.template")?;

            Self::inject_entity_to_pod_spec(entity, pod_spec)?;

            Ok((base_name, serde_yaml::to_string(&deployment)?))
        } else if let Ok(mut pod) = serde_yaml::from_str::<Pod>(&data) {
            let pod_spec = pod.spec.as_mut().context("missing spec in pod")?;

            Self::inject_entity_to_pod_spec(entity, pod_spec)?;

            Ok((base_name, serde_yaml::to_string(&pod)?))
        } else {
            panic!("Unknown resource type")
        }
    }

    pub fn inject_entities(
        entities: Vec<Entity>,
        mapping: &HashMap<String, PathBuf>,
    ) -> Result<Vec<(String, String)>, anyhow::Error> {
        let specs = entities
            .into_iter()
            .filter(|entity| !entity.requires.is_empty() || !entity.excludes.is_empty())
            .map(|entity| {
                let path = mapping.get(entity.name.as_ref()).with_context(|| {
                    format!("No source file found for entity {}", entity.name.as_ref())
                })?;

                Self::inject_entity(entity, path)
            })
            .collect::<Vec<_>>();

        let specs = specs.into_iter().collect::<Result<Vec<_>, _>>()?;

        Ok(specs)
    }

    pub fn remove_rule_from_pod_spec(
        entity: Entity,
        rules: &HashSet<usize>,
        pod_spec: &mut PodSpec,
    ) -> anyhow::Result<()> {
        let affinity = pod_spec.affinity.as_mut();

        if let Some(affinity) = affinity {
            let pod_affinity = affinity.pod_affinity.as_mut();

            if let Some(pod_affinity) = pod_affinity {
                let terms = pod_affinity
                    .required_during_scheduling_ignored_during_execution
                    .take();

                let terms = if let Some(terms) = terms {
                    Some(
                        terms
                            .into_iter()
                            .filter(|e| !rules.contains(&e.line))
                            .collect(),
                    )
                } else {
                    None
                };

                pod_affinity.required_during_scheduling_ignored_during_execution = terms;
            }

            let pod_anti_affinity = affinity.pod_anti_affinity.as_mut();
            if let Some(pod_anti_affinity) = pod_anti_affinity {
                let terms = pod_anti_affinity
                    .required_during_scheduling_ignored_during_execution
                    .take();

                let terms = if let Some(terms) = terms {
                    Some(
                        terms
                            .into_iter()
                            .filter(|e| !rules.contains(&e.line))
                            .collect(),
                    )
                } else {
                    None
                };

                pod_anti_affinity.required_during_scheduling_ignored_during_execution = terms;
            }

            if let Some(node_affinity) = affinity.node_affinity.as_mut() {
                let terms = node_affinity
                    .required_during_scheduling_ignored_during_execution
                    .as_mut()
                    .context("Invalid node affinity")?;

                let terms = &mut terms.node_selector_terms;

                *terms = terms
                    .iter()
                    .filter(|e| !rules.contains(&e.line))
                    .cloned()
                    .collect();
            }
        }

        Ok(())
    }

    pub fn remove_rule_from_entity(
        entity: Entity,
        rules: &HashSet<(String, usize)>,
        path: &Path,
    ) -> anyhow::Result<(String, String)> {
        let base_name = path.file_name().context("No file name found")?;
        let base_name = base_name.to_str().context("Invalid file name")?;
        let base_name = base_name.to_string();

        let data = std::fs::read_to_string(path)?;
        let path_string = path.display().to_string();
        let line_numbers = rules
            .iter()
            .filter(|(file, _)| file.as_str() == &path_string)
            .map(|(_, line)| *line)
            .collect::<HashSet<_>>();

        debug!(
            "Removing rules from entity: {:?}, {:?}",
            entity, line_numbers
        );

        if let Ok(mut deployment) = serde_yaml::from_str::<Deployment>(&data) {
            let pod_spec = deployment
                .spec
                .as_mut()
                .context("missing spec in deployment")?
                .template
                .spec
                .as_mut()
                .context("missing spec in deployment.template")?;

            Self::remove_rule_from_pod_spec(entity, &line_numbers, pod_spec)?;

            Ok((base_name, serde_yaml::to_string(&deployment)?))
        } else if let Ok(mut pod) = serde_yaml::from_str::<Pod>(&data) {
            let pod_spec = pod.spec.as_mut().context("missing spec in pod")?;

            Self::remove_rule_from_pod_spec(entity, &line_numbers, pod_spec)?;

            Ok((base_name, serde_yaml::to_string(&pod)?))
        } else {
            panic!("Unknown resource type")
        }
    }

    pub fn id_entity(path: &Path) -> anyhow::Result<(String, String)> {
        let base_name = path.file_name().context("No file name found")?;
        let base_name = base_name.to_str().context("Invalid file name")?;
        let base_name = base_name.to_string();

        let data = std::fs::read_to_string(path)?;

        if let Ok(deployment) = serde_yaml::from_str::<Deployment>(&data) {
            Ok((base_name, serde_yaml::to_string(&deployment)?))
        } else if let Ok(pod) = serde_yaml::from_str::<Pod>(&data) {
            Ok((base_name, serde_yaml::to_string(&pod)?))
        } else {
            panic!("Unknown resource type")
        }
    }

    pub fn remove_rules_from_entities(
        entities: Vec<Entity>,
        rules: &[EntityRule],
        mapping: &HashMap<String, PathBuf>,
    ) -> Result<Vec<(String, String)>, anyhow::Error> {
        let file_name_and_lines = rules.iter().fold(HashSet::new(), |mut acc, rule| {
            let source = rule.file().map(|e| e.to_string());
            let line = rule.line();

            match (source, line) {
                (Some(source), Some(line)) => {
                    acc.insert((source, line));
                }
                _ => {}
            }

            acc
        });

        let files = file_name_and_lines
            .iter()
            .map(|e| e.0.clone())
            .collect::<HashSet<_>>();

        let specs = entities
            .into_iter()
            .filter(|entity| !entity.requires.is_empty() || !entity.excludes.is_empty())
            .map(|entity| {
                let path = mapping.get(entity.name.as_ref()).with_context(|| {
                    format!("No source file found for entity {}", entity.name.as_ref())
                })?;

                let path_string = path.display().to_string();

                match files.contains(&path_string) {
                    false => {
                        debug!(
                            "Entity {} is not found in the mapping, assuming it's a dummy entity, path: {}, {:?}",
                            entity.name.as_ref(),
                            path_string,
                            rules
                        );
                        Self::id_entity(path)
                    }
                    true => Self::remove_rule_from_entity(entity, &file_name_and_lines, path),
                }
            })
            .collect::<Vec<_>>();

        let specs = specs.into_iter().collect::<Result<Vec<_>, _>>()?;

        Ok(specs)
    }
}
