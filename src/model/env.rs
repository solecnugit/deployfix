use std::collections::{HashMap, HashSet};

#[derive(Debug, thiserror::Error)]
pub enum EnvParseError {
    #[error("Empty env data")]
    EmptyData,
}

#[derive(Debug, Clone)]
pub struct Env {
    pub name: String,
    pub labels: Vec<String>,
    pub duplicate_names: Vec<String>,
}

pub trait EnvParser {
    fn parse(&self, data: &str) -> Result<Vec<Env>, EnvParseError>;
}

pub struct DefaultEnvParser {}

impl EnvParser for DefaultEnvParser {
    // format:
    // env_name app=app1;app=app2;app=app3;node=high-performance-node;
    fn parse(&self, data: &str) -> Result<Vec<Env>, EnvParseError> {
        let envs = data
            .lines()
            .filter_map(|line| {
                if line.is_empty() {
                    return None;
                }

                let parts = line.split_whitespace().collect::<Vec<_>>();
                let env_name = parts[0].to_string();

                let labels = if parts.len() < 2 {
                    vec![]
                } else {
                    let mut labels: Vec<String> = parts[1]
                        .split(';')
                        .filter_map(|s| {
                            if s.is_empty() {
                                None
                            } else {
                                Some(s.to_string())
                            }
                        })
                        .collect();
                    labels.sort();

                    labels
                };

                Some((env_name, labels))
            })
            .collect::<HashMap<String, Vec<String>>>();

        // group by label groups
        let mut seen_envs: HashMap<Vec<String>, Env> = HashMap::new();

        for (name, labels) in envs {
            if seen_envs.contains_key(&labels) {
                let env = seen_envs.get_mut(&labels).unwrap();
                env.duplicate_names.push(name);
            } else {
                let env = Env {
                    name,
                    labels: labels.clone(),
                    duplicate_names: vec![],
                };
                seen_envs.insert(labels, env);
            }
        }

        let envs: Vec<Env> = seen_envs.into_iter().map(|(_, v)| v).collect();
        if envs.is_empty() {
            return Err(EnvParseError::EmptyData);
        }

        Ok(envs)
    }
}
