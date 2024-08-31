use std::collections::{HashMap, HashSet};

use crate::model::{EntityName, EntityRule, Env};

use super::{map::EntityMap, solver::Solver, SolverOutput};

pub struct UnknownSolver;

impl UnknownSolver {
    pub fn new() -> Self {
        Self
    }

    fn collect_definitions(&self, entities: &EntityMap) -> HashSet<EntityName> {
        let mut known_definitions = HashSet::new();

        for entity in entities.entities.iter() {
            known_definitions.insert(entity.name.clone());
        }

        known_definitions
    }
}

impl Solver<'_> for UnknownSolver {
    fn solve(&self, entities: &super::map::EntityMap) -> SolverOutput {
        let known_definitions = self.collect_definitions(entities);

        let conflicts = entities
            .entities
            .iter()
            .filter_map(|e| {
                let rules = e.rules();
                let unknown_rules = rules
                    .into_iter()
                    .filter(|e| match e {
                        EntityRule::Mono { target, .. } => !known_definitions.contains(target),
                        EntityRule::Multi { targets, .. } => {
                            targets.iter().any(|t| !known_definitions.contains(t))
                        }
                    })
                    .cloned()
                    .collect::<Vec<_>>();

                if unknown_rules.is_empty() {
                    None
                } else {
                    Some((e.name.0.clone(), unknown_rules))
                }
            })
            .collect::<HashMap<_, _>>();

        if conflicts.is_empty() {
            SolverOutput::Ok
        } else {
            SolverOutput::Conflict(conflicts)
        }
    }

    fn set_envs(&self, envs: Vec<Env>) {
        unreachable!()
    }
}
