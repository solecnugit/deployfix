use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
};

use anyhow::Context;
use clap::Subcommand;
use log::{debug, error, info, warn};

use crate::{
    cli::ConflictAnnotater,
    model::{
        get_parser, merge_entities, DeployIRFormatter, Entity, EntityPriority, EntityRule,
        EntitySource, EnvParser,
    },
    solver::{get_solver, SolverOutput},
    util,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendPolicy {
    HighPriorityFirst,
    All,
}

impl Default for RecommendPolicy {
    fn default() -> Self {
        RecommendPolicy::HighPriorityFirst
    }
}

impl From<&str> for RecommendPolicy {
    fn from(s: &str) -> Self {
        match s {
            "HighPriorityFirst" => RecommendPolicy::HighPriorityFirst,
            "All" => RecommendPolicy::All,
            _ => panic!("Invalid recommend policy"),
        }
    }
}

#[derive(Subcommand)]
pub enum K8SCommands {
    Import {
        #[clap(value_name = "PATH", help = "Paths to K8s files")]
        paths: Vec<PathBuf>,
    },
    Inject {
        #[clap(value_name = "OUTPUT", help = "Output K8s directory")]
        output_dir: PathBuf,
        #[clap(value_name = "PATH", help = "Paths to deployfix files")]
        paths: Vec<PathBuf>,
    },
    Go {
        #[clap(value_name = "SOURCE_DIR", help = "Path to K8s files")]
        source_dir: PathBuf,
        #[clap(value_name = "INJECTION_DIR", help = "Path to deployfix files")]
        inject_dir: PathBuf,
        #[clap(value_name = "OUTPUT", help = "Path to output")]
        output_dir: PathBuf,
        #[clap(
            long,
            short,
            help = "Recommend possible solution when unsatisfiable",
            default_value = "false"
        )]
        recommend: bool,
        #[clap(
            long,
            help = "Recommand policy to use",
            default_value = "HighPriorityFirst"
        )]
        recommend_policy: RecommendPolicy,
        #[clap(long, help = "Enviroment file")]
        env_file: Option<PathBuf>,
        #[clap(long, help = "Enable cycle check", default_value = "false")]
        cycle_check: bool,
        #[clap(long, help = "Reject unknown entities", default_value = "false")]
        reject_unknown: bool,
    },
}

fn dump_recommendation_to_file(recommendations: &[EntityRule], output: &Path) {
    let recommendations = recommendations
        .iter()
        .map(|rule| {
            let file = rule.file().unwrap_or("Unknown");
            let line = rule.line().unwrap_or(0);

            format!("{}:{}", file, line)
        })
        .collect::<Vec<_>>();

    let recommendations = serde_yaml::to_string(&recommendations).unwrap();
    let target_file = output.join("recommendations.yaml");

    if target_file.exists() {
        std::fs::remove_file(&target_file).expect("Failed to remove old recommendations file");

        warn!(
            "Removed old recommendations file {} before writing new one",
            target_file.display()
        );
    }

    std::fs::write(&target_file, recommendations).expect("Failed to write recommendations to file");
    info!("Dumped recommendations to {}", target_file.display());
}

fn dump_conflicts_to_file(
    conflicts: &HashMap<String, Vec<EntityRule>>,
    output: &Path,
    topology: &str,
) {
    /*
       Format:
       UnscheableEntities:
           - A:
               - FileName:Line
           - B
               - FileName:Line
               - FileName:Line
           - C
               - FileName:Line
    */
    #[derive(serde::Serialize)]
    struct Conflict {
        name: String,
        conflicts: Vec<String>,
    }

    #[derive(serde::Serialize)]
    struct ConflictFile {
        unscheduable_entities: Vec<Conflict>,
    }

    let conflicts = conflicts
        .iter()
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .map(|(name, rules)| {
            let conflicts = rules
                .iter()
                .map(|rule| {
                    let file = rule.file().unwrap_or("Unknown");
                    let line = rule.line().unwrap_or(0);

                    format!("{}:{}", file, line)
                })
                .collect();

            Conflict {
                name: name.clone(),
                conflicts,
            }
        })
        .collect();

    let conflicts = ConflictFile {
        unscheduable_entities: conflicts,
    };

    let conflicts = serde_yaml::to_string(&conflicts).unwrap();
    let target_file = output.join(format!("conflicts-{}.yaml", topology));

    if target_file.exists() {
        std::fs::remove_file(&target_file).expect("Failed to remove old conflicts file");

        warn!(
            "Removed old conflicts file {} before writing new one",
            target_file.display()
        );
    }

    std::fs::write(&target_file, conflicts).expect("Failed to write conflicts to file");
    info!("Dumped conflicts to {}", target_file.display());
}

pub fn execute(command: K8SCommands) {
    match command {
        K8SCommands::Import { paths } => {
            let entities = paths
                .iter()
                .filter_map(|path| {
                    debug!("Importing from {}", path.display());

                    let entity = crate::plugin::k8s::K8sPlugin::extract_entity_from_path(path);

                    match entity {
                        Ok(entity) => {
                            debug!("Imported entity {:?} from {}", entity, path.display());

                            Some(entity)
                        }
                        Err(err) => {
                            warn!("Failed to extract entity from {}: {}", path.display(), err);
                            None
                        }
                    }
                })
                .flatten()
                .collect::<Vec<_>>();

            match entities.is_empty() {
                true => {
                    warn!("No entities found");
                    std::process::exit(1);
                }
                false => {}
            }

            let output = DeployIRFormatter::format(&entities);
            info!("{}", output);

            std::fs::write("output.ir", output).unwrap();
        }
        K8SCommands::Inject { output_dir, paths } => {
            let entities = paths
                .iter()
                .flat_map(|path| {
                    debug!("Importing from {}", path.display());

                    get_parser("deployfix")
                        .unwrap()
                        .parse(
                            &std::fs::read_to_string(path).unwrap(),
                            crate::model::EntitySource::File(path.to_str().unwrap().to_string()),
                        )
                        .expect("Failed to parse deployfix file")
                })
                .collect::<Vec<_>>();

            let entities = merge_entities(
                entities,
                Some(|a, b| match (a, b) {
                    (EntitySource::File(a), EntitySource::File(b)) => {
                        if !a.ends_with(".yaml") {
                            warn!("Replacing {} with {}", a, b);
                            *a = b;
                        }
                    }
                    _ => {}
                }),
            );

            debug!("Imported entities: {:?}", entities);

            inject(entities, &output_dir)
        }
        K8SCommands::Go {
            source_dir,
            inject_dir,
            output_dir,
            recommend,
            recommend_policy,
            env_file,
            cycle_check,
            reject_unknown,
        } => {
            let k8s_entities = std::fs::read_dir(&source_dir)
                .with_context(|| {
                    format!(
                        "Failed to read source directory: {}",
                        source_dir.display().to_string()
                    )
                })
                .unwrap()
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let file_name = entry.file_name().to_str().unwrap().to_string();
                    let file_path = &entry.path();

                    if file_name.ends_with(".yaml") {
                        let entity =
                            crate::plugin::k8s::K8sPlugin::extract_entity_from_path(file_path);

                        match entity {
                            Ok(entity) => return Some(entity),
                            Err(err) => {
                                warn!("Failed to extract entity from {}: {}", file_name, err);
                                return None;
                            }
                        }
                    }

                    None
                })
                .flatten();

            let deployfix_entities = std::fs::read_dir(inject_dir);
            let deployfix_entities = match deployfix_entities {
                Ok(deployfix_entities) => deployfix_entities.into_iter().collect::<Vec<_>>(),
                Err(err) => {
                    warn!("Failed to read inject directory: {}", err);
                    vec![]
                }
            };

            let deployfix_entities = deployfix_entities
                .into_iter()
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let file_name = entry.file_name().to_str().unwrap().to_string();
                    let file_path = &entry.path();

                    if file_name.ends_with(".ir") {
                        let entities = get_parser("deployfix")
                            .unwrap()
                            .parse(
                                &std::fs::read_to_string(file_path).unwrap(),
                                crate::model::EntitySource::File(
                                    file_path.to_str().unwrap().to_string(),
                                ),
                            )
                            .unwrap();

                        return Some(entities);
                    }

                    None
                })
                .flatten()
                .collect::<Vec<_>>();

            let has_injected_flag = !deployfix_entities.is_empty();

            let entities = k8s_entities.chain(deployfix_entities).collect::<Vec<_>>();
            let entities = merge_entities(
                entities,
                Some(|a, b| match (a, b) {
                    (EntitySource::File(a), EntitySource::File(b)) => {
                        if !a.ends_with(".yaml") {
                            warn!("Replacing {} with {}", a, b);
                            *a = b;
                        }
                    }
                    _ => {}
                }),
            );

            debug!("Imported Entities {:?}", entities);

            // Dump entities
            let output = DeployIRFormatter::format(&entities);
            std::fs::create_dir_all(&output_dir).unwrap();
            std::fs::write(output_dir.join("dump.ir"), output).unwrap();

            let definitions = dump_definitions(&entities);
            std::fs::write(output_dir.join("definitions.yaml"), definitions).unwrap();

            // Split entities by different topologyKeys
            let topology_split_entities = split_entities_by_topo_key(&entities);

            let envs = if let Some(env_file) = env_file {
                let env_data = std::fs::read_to_string(env_file).unwrap();
                let env_parser = crate::model::DefaultEnvParser {};
                env_parser.parse(&env_data).ok()
            } else {
                None
            };

            let mut has_conflict = false;
            for (key, entities) in topology_split_entities {
                info!("Checking topology: {}", key);

                let entity_map = (&entities).try_into().unwrap();

                std::fs::write(
                    output_dir.join(format!("dump-{key}.yaml")),
                    serde_yaml::to_string(&entity_map).unwrap(),
                )
                .unwrap();

                let result = {
                    let z3_solver = get_solver("z3").unwrap();
                    if let Some(envs) = &envs {
                        z3_solver.set_envs(envs.clone());
                    }

                    let mut result = z3_solver.solve(&entity_map);
                    if cycle_check {
                        let ring_solver = get_solver("ring").unwrap();
                        let ring_result = ring_solver.solve(&entity_map);

                        result = result.merge(ring_result);
                    }
                    if reject_unknown {
                        let unknown_solver = get_solver("unknown").unwrap();
                        let unknown_result = unknown_solver.solve(&entity_map);

                        result = result.merge(unknown_result);
                    }
                    result
                };

                // let result = if cycle_check {
                //     let ring_solver = get_solver("ring").unwrap();
                //     let ring_result = ring_solver.solve(&entity_map);

                //     let z3_solver = get_solver("z3").unwrap();
                //     let z3_result = z3_solver.solve(&entity_map);

                //     ring_result.merge(z3_result)
                // } else {
                //     let z3_solver = get_solver("z3").unwrap();
                //     z3_solver.solve(&entity_map)
                // };

                if let SolverOutput::Conflict(conflicts) = result {
                    {
                        if recommend {
                            let recommendations = match recommend_policy {
                                RecommendPolicy::HighPriorityFirst => {
                                    let priority_map = conflicts
                                        .keys()
                                        .into_iter()
                                        .map(|e| {
                                            (
                                                e,
                                                entity_map
                                                    .entities
                                                    .iter()
                                                    .find(|x| x.name.0.as_str() == e)
                                                    .unwrap()
                                                    .priority
                                                    .clone(),
                                            )
                                        })
                                        .collect::<HashMap<_, _>>();

                                    recommend_policy_high_priority_first(&priority_map, &conflicts)
                                }
                                RecommendPolicy::All => recommend_policy_all(&conflicts),
                            };

                            let recommendations = if recommendations.is_empty() {
                                warn!("No recommendations found for high priority first, using default strategy");

                                recommend_policy_all(&conflicts)
                            } else {
                                recommendations
                            };

                            dump_recommendation_to_file(&recommendations, &output_dir);

                            let output_solution_dir = output_dir.join("solution");

                            remove_rules_from_entities(
                                entities,
                                &recommendations,
                                &output_solution_dir,
                            );
                        }
                    }

                    {
                        let base_topo_key = if key.contains('/') {
                            key.split('/').last().unwrap()
                        } else {
                            key.as_str()
                        };

                        dump_conflicts_to_file(&conflicts, &output_dir, base_topo_key);
                    }

                    let conflicts_annotations = conflicts
                        .into_iter()
                        .flat_map(|(k, v)| v.into_iter().map(move |v| (k.clone(), v)))
                        .map(|(name, rule)| ConflictAnnotater::new(name.as_str(), &rule).annotate())
                        .collect::<Vec<_>>();

                    let conflicts_output = conflicts_annotations.join("\n\n");

                    error!("{}", conflicts_output);

                    has_conflict = true;
                }
            }

            if has_conflict {
                error!("Conflicts found, aborting");
                std::process::exit(1);
            }

            info!("No conflicts found");

            if !has_injected_flag {
                info!("No injected entities found, aborting");
            } else {
                info!("Injecting entities");
                inject(entities, &output_dir);
            }
        }
    }
}

fn inject(entities: Vec<Entity>, output_dir: &Path) {
    let mapping = crate::plugin::k8s::K8sPlugin::scan_entity_file_mapping(&entities)
        .expect("Failed to scan entity file mapping");
    let pods = crate::plugin::k8s::K8sPlugin::inject_entities(entities, &mapping)
        .expect("Failed to inject entities");

    for (base_name, spec) in pods {
        // let output = serde_yaml::to_string(&pod).unwrap();
        // let name = pod.metadata.name.unwrap();
        // let name = format!("app={}", name);
        // let path = mapping.get(&base_name).unwrap();
        // let file_name = path.file_name().unwrap();
        let output_path = output_dir.join(base_name);

        std::fs::create_dir_all(output_path.parent().unwrap()).expect("Failed to create dir");
        std::fs::write(output_path, spec).expect("Failed to write file");
    }
}

fn remove_rules_from_entities(entities: Vec<Entity>, rules: &[EntityRule], output_dir: &Path) {
    let mapping = crate::plugin::k8s::K8sPlugin::scan_entity_file_mapping(&entities)
        .expect("Failed to scan entity file mapping");
    let pods = crate::plugin::k8s::K8sPlugin::remove_rules_from_entities(entities, rules, &mapping)
        .expect("Failed to remove entities");

    for (base_name, spec) in pods {
        let output_path = output_dir.join(base_name);

        std::fs::create_dir_all(output_path.parent().unwrap()).expect("Failed to create dir");
        std::fs::write(output_path, spec).expect("Failed to write file");
    }
}

fn split_entities_by_topo_key(entities: &[Entity]) -> HashMap<String, Vec<Entity>> {
    util::split_by_metadata(entities, "topology", "node")
}

fn recommend_policy_high_priority_first(
    priority_map: &HashMap<&String, EntityPriority>,
    conflicts: &HashMap<String, Vec<EntityRule>>,
) -> Vec<EntityRule> {
    let critical_apps = priority_map
        .iter()
        .filter_map(|(k, v)| {
            if *v == EntityPriority::Critical {
                Some(k.as_str())
            } else {
                None
            }
        })
        .collect::<HashSet<_>>();

    let critical_conflicts = conflicts
        .iter()
        .filter_map(|(k, v)| {
            if critical_apps.contains(k.as_str()) {
                Some(v)
            } else {
                None
            }
        })
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();

    return critical_conflicts;
}

fn recommend_policy_all(conflicts: &HashMap<String, Vec<EntityRule>>) -> Vec<EntityRule> {
    let unique_rule_set = conflicts
        .values()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let unique_rule_set_count = unique_rule_set.len();

    debug!("Unique rule set count: {:?}", unique_rule_set_count);

    let mut rule_count = unique_rule_set
        .iter()
        .fold(HashMap::new(), |mut acc, e| {
            for rule in *e {
                let count = acc.entry(rule).or_insert(0);
                *count += 1;
            }

            acc
        })
        .into_iter()
        .collect::<Vec<_>>();

    rule_count.sort_by(|a, b| b.1.cmp(&a.1));

    debug!("Conflict order: {:?}", rule_count);

    let (rules, _) = rule_count
        .into_iter()
        .fold((Vec::new(), 0), |(mut ret, mut sum), (e, _)| {
            let relation_cnt = match e {
                EntityRule::Mono { .. } => 1,
                EntityRule::Multi { targets, .. } => targets.len(),
            };

            if sum < unique_rule_set_count {
                ret.push(e.clone());
            }

            sum += relation_cnt;

            (ret, sum)
        });

    debug!("Recommendation: {:?}", rules);

    rules
}

enum DefinitionEntry {
    Source {
        name: String,
        file: String,
    },
    Reference {
        name: String,
        file: String,
        line: usize,
    },
}

fn dump_definition(entity: &Entity) -> Vec<DefinitionEntry> {
    let name = entity.name.0.clone();
    let source = entity.source.as_ref().to_string();

    let mut ret = vec![DefinitionEntry::Source { name, file: source }];

    for rule in entity.rules() {
        match rule {
            EntityRule::Mono {
                source,
                target,
                r#type,
                rule_source,
                metadata,
            } => {
                let name = target.0.clone();
                let file = rule_source.file().unwrap_or("unknown").to_string();
                let line = rule_source.line().unwrap_or(0);

                ret.push(DefinitionEntry::Reference { name, file, line });
            }
            EntityRule::Multi {
                source,
                targets,
                r#type,
                rule_source,
                metadata,
            } => {
                for target in targets {
                    let name = target.0.clone();
                    let file = rule_source.file().unwrap_or("unknown").to_string();
                    let line = rule_source.line().unwrap_or(0);

                    ret.push(DefinitionEntry::Reference { name, file, line });
                }
            }
        }
    }

    ret
}

#[derive(serde::Serialize)]
struct Definition {
    name: String,
    source: String,
    references: Vec<String>,
}

fn dump_definitions(entities: &[Entity]) -> String {
    let definitions = entities
        .iter()
        .map(|e| dump_definition(e))
        .flatten()
        .collect::<Vec<_>>();

    let (sources, references) = definitions.into_iter().fold(
        (Vec::new(), Vec::new()),
        |(mut sources, mut references), e| {
            if matches!(e, DefinitionEntry::Source { .. }) {
                sources.push(e);
            } else {
                references.push(e);
            }

            (sources, references)
        },
    );

    let mut definitions =
        sources
            .into_iter()
            .fold(HashMap::<String, Definition>::new(), |mut acc, e| {
                if let DefinitionEntry::Source { name, file } = e {
                    let d = Definition {
                        name: name.clone(),
                        source: file,
                        references: vec![],
                    };

                    if acc.contains_key(&name) {
                        panic!("Duplicate definition found: {}", name);
                    } else {
                        acc.insert(name, d);
                    }

                    acc
                } else {
                    unreachable!()
                }
            });

    for e in references {
        if let DefinitionEntry::Reference { name, file, line } = e {
            if let Some(d) = definitions.get_mut(&name) {
                d.references.push(format!("{}:{}", file, line));
            } else {
                definitions.insert(
                    name.clone(),
                    Definition {
                        name: name.clone(),
                        source: "unknown".to_string(),
                        references: vec![format!("{}:{}", file, line)],
                    },
                );
            }
        } else {
            unreachable!()
        }
    }

    let sources = definitions.into_iter().map(|e| e.1).collect::<Vec<_>>();
    let sources = serde_yaml::to_string(&sources).unwrap();

    sources
}
