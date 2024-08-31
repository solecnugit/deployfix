use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    pin::Pin,
    sync::atomic::AtomicBool,
};

use thiserror::Error;

use crate::model::{EntityRule, Env};

use super::{map::EntityMap, ring::RingSolver, unknown::UnknownSolver, z3::Z3Solver};

#[derive(Debug)]
pub enum SolverOutput {
    Ok,
    Conflict(HashMap<String, Vec<EntityRule>>),
}

impl SolverOutput {
    pub fn new_ok() -> Self {
        Self::Ok
    }

    pub fn new_conflict(conflicts: HashMap<String, Vec<EntityRule>>) -> Self {
        let conflicts = conflicts
            .into_iter()
            .map(|(name, mut rules)| {
                rules.sort();
                rules.dedup();
                (name, rules)
            })
            .collect();

        Self::Conflict(conflicts)
    }

    pub fn merge(self, other: Self) -> SolverOutput {
        match (self, other) {
            (Self::Ok, Self::Ok) => {
                // Do nothing
                SolverOutput::Ok
            }
            (Self::Ok, Self::Conflict(conflicts)) => SolverOutput::Conflict(conflicts),
            (Self::Conflict(conflicts), Self::Ok) => {
                // Do nothing
                SolverOutput::Conflict(conflicts)
            }
            (Self::Conflict(conflicts), Self::Conflict(other_conflicts)) => {
                let mut merged_conflicts = conflicts;
                for (name, rules) in other_conflicts {
                    if let Some(existing) = merged_conflicts.get_mut(&name) {
                        existing.extend(rules);
                        existing.sort();
                        existing.dedup();
                    } else {
                        merged_conflicts.insert(name, rules);
                    }
                }

                SolverOutput::Conflict(merged_conflicts)
            }
        }
    }

    pub fn is_ok(&self) -> bool {
        match self {
            SolverOutput::Ok => true,
            SolverOutput::Conflict(_) => false,
        }
    }

    pub fn is_conflict(&self) -> bool {
        match self {
            SolverOutput::Ok => false,
            SolverOutput::Conflict(_) => true,
        }
    }

    pub fn get_unscheduable(&self) -> Option<HashSet<String>> {
        match self {
            SolverOutput::Ok => None,
            SolverOutput::Conflict(conflicts) => Some(conflicts.keys().cloned().collect()),
        }
    }

    pub fn get_conflict_rules(&self) -> Option<HashMap<String, Vec<EntityRule>>> {
        match self {
            SolverOutput::Ok => None,
            SolverOutput::Conflict(conflicts) => Some(conflicts.clone()),
        }
    }
}

impl Display for SolverOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolverOutput::Ok => write!(f, "SolverResult::Ok"),
            SolverOutput::Conflict(conflicts) => {
                for (name, sources) in conflicts.iter() {
                    writeln!(f, "Unscheduable: {}", name)?;
                    writeln!(f, "  Conflicts: ")?;
                    for source in sources.iter() {
                        writeln!(f, "  {}", source)?;
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum SolverError {
    #[error("Unknown solver: {0}")]
    UnknownSolver(String),
}

pub trait Solver<'instance> {
    fn solve(&'instance self, entities: &EntityMap) -> SolverOutput;

    fn set_envs(&'instance self, envs: Vec<Env>);
}

pub struct SolverImpl {
    name: String,
    solver: Pin<Box<dyn for<'a> Solver<'a>>>,
}

impl SolverImpl {
    pub fn solve(&self, entities: &EntityMap) -> SolverOutput {
        self.solver.solve(entities)
    }

    pub fn set_envs(&self, envs: Vec<Env>) {
        let inner = Pin::as_ref(&self.solver);

        inner.set_envs(envs);
    }
}

pub fn get_solver(name: &str) -> Result<SolverImpl, SolverError> {
    match name {
        "z3" => {
            let solver = Z3Solver::new();
            let solver = unsafe {
                std::mem::transmute::<Pin<Box<dyn Solver<'_>>>, Pin<Box<dyn for<'a> Solver<'a>>>>(
                    solver,
                )
            };

            Ok(SolverImpl {
                name: name.to_string(),
                solver,
            })
        }
        "ring" => {
            let solver = Box::pin(RingSolver::new());
            let solver = unsafe {
                std::mem::transmute::<Pin<Box<dyn Solver<'_>>>, Pin<Box<dyn for<'a> Solver<'a>>>>(
                    solver,
                )
            };

            Ok(SolverImpl {
                name: name.to_string(),
                solver,
            })
        }
        "unknown" => {
            let solver = Box::pin(UnknownSolver::new());
            let solver = unsafe {
                std::mem::transmute::<Pin<Box<dyn Solver<'_>>>, Pin<Box<dyn for<'a> Solver<'a>>>>(
                    solver,
                )
            };

            Ok(SolverImpl {
                name: name.to_string(),
                solver,
            })
        }
        _ => Err(SolverError::UnknownSolver(name.to_string())),
    }
}
