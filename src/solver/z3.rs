use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    pin::Pin,
};

use log::{debug, warn};
use z3::{Config, Context};

use crate::model::{EntityRule, Env};

use super::{
    map::EntityMap,
    solver::{self, Solver, SolverOutput},
};
pub struct Z3Solver<'ctx> {
    vars: RefCell<HashMap<String, z3::ast::Bool<'ctx>>>,
    rule_trackers: RefCell<HashMap<String, z3::ast::Bool<'ctx>>>,
    rule_mapping: RefCell<HashMap<String, EntityRule>>,
    self_conflicts: RefCell<HashMap<String, z3::ast::Bool<'ctx>>>,
    ctx: Context,
    envs: RefCell<Option<Vec<Env>>>,
    _unpin: std::marker::PhantomPinned,
}

impl<'ctx> Z3Solver<'ctx> {
    pub fn new() -> Pin<Box<Self>> {
        let config = Config::new();
        let cpus = num_cpus::get();
        // enable parallelism
        z3::set_global_param("parallel.enable", "true");
        z3::set_global_param("parallel.threads.max", cpus.to_string().as_str());

        let ctx = Context::new(&config);

        let inner = Self {
            ctx,
            vars: RefCell::new(HashMap::new()),
            self_conflicts: RefCell::new(HashMap::new()),
            rule_trackers: RefCell::new(HashMap::new()),
            rule_mapping: RefCell::new(HashMap::new()),
            envs: RefCell::new(None),
            _unpin: std::marker::PhantomPinned,
        };

        Box::pin(inner)
    }

    fn get_or_create_bool(&'ctx self, name: &str) -> z3::ast::Bool<'ctx> {
        let mut vars = RefCell::borrow_mut(&self.vars);

        vars.entry(name.to_string())
            .or_insert_with(|| z3::ast::Bool::new_const(&self.ctx, name))
            .clone()
    }

    fn create_rule_tracker(&'ctx self, rule: &EntityRule) -> z3::ast::Bool<'ctx> {
        let source_string = format!("{}", rule);

        let mut trackers = RefCell::borrow_mut(&self.rule_trackers);
        let mut mapping = RefCell::borrow_mut(&self.rule_mapping);

        mapping.insert(source_string.clone(), rule.clone());

        trackers
            .entry(source_string.clone())
            .or_insert_with(|| z3::ast::Bool::new_const(&self.ctx, source_string))
            .clone()
    }

    fn require(&'ctx self, a: &str, b: &str) -> z3::ast::Bool<'ctx> {
        let a = self.get_or_create_bool(a);
        let b = self.get_or_create_bool(b);

        a.implies(&b)
    }

    fn conflict(&'ctx self, a: &str, b: &str) -> z3::ast::Bool<'ctx> {
        let a = self.get_or_create_bool(a);
        let b = self.get_or_create_bool(b);

        z3::ast::Bool::or(&self.ctx, &[&a.not(), &b.not()])
    }

    fn track(
        &'ctx self,
        solver: &z3::Solver,
        rule: &z3::ast::Bool<'ctx>,
        entity_rule: &EntityRule,
    ) {
        let tracker = self.create_rule_tracker(entity_rule);

        solver.assert_and_track(rule, &tracker);
    }

    fn check_and_get(&'ctx self, solver: &mut z3::Solver) -> Option<Vec<EntityRule>> {
        match solver.check() {
            z3::SatResult::Sat => {
                debug!("Solver result: {:?}", solver.get_model());

                None
            }
            z3::SatResult::Unsat => {
                let unsat_core = solver
                    .get_unsat_core()
                    .iter()
                    .filter_map(|r| {
                        let source_string = r
                            .to_string()
                            .trim_matches('|')
                            .replace("\\|", "|")
                            .to_string();
                        let mapping = RefCell::borrow(&self.rule_mapping);

                        // Ignore self-conflict assumptions injected
                        mapping.get(&source_string).cloned()
                    })
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();

                debug!("Unsat Core: {:?}", unsat_core);

                {
                    let output = solver
                        .get_unsat_core()
                        .iter()
                        .filter_map(|r| {
                            let source_string = r
                                .to_string()
                                .trim_matches('|')
                                .replace("\\|", "|")
                                .to_string();
                            let mapping = RefCell::borrow(&self.self_conflicts);

                            // Ignore non-self-conflict assumptions injected
                            mapping.get(&source_string).cloned()
                        })
                        .collect::<Vec<_>>();

                    debug!("Self-conflict assumptions: {:?}", output);
                }

                Some(unsat_core)
            }
            z3::SatResult::Unknown => {
                unreachable!()
            }
        }
    }
}

impl<'ctx> Solver<'ctx> for Z3Solver<'ctx> {
    fn solve(&'ctx self, map: &EntityMap) -> SolverOutput {
        let mut solver = z3::Solver::new(&self.ctx);

        for entity in map.entities.iter().filter(|e| !e.is_dummy()) {
            let name = entity.name.as_ref();
            let requires = &entity.requires;

            for require in requires.iter() {
                match require {
                    EntityRule::Mono { target: rule, .. } => {
                        let rule = self.require(name, &rule.0);
                        self.track(&solver, &rule, require);
                    }
                    EntityRule::Multi { targets: rules, .. } => {
                        let rules = rules
                            .iter()
                            .map(|r| self.require(name, &r.0))
                            .collect::<Vec<_>>();

                        let rule = z3::ast::Bool::or(&self.ctx, &rules.iter().collect::<Vec<_>>());
                        self.track(&solver, &rule, require);
                    }
                }
            }

            let excludes = &entity.excludes;
            for exclude in excludes.iter() {
                match exclude {
                    EntityRule::Mono { target: rule, .. } => {
                        let rule = self.conflict(name, &rule.0);
                        self.track(&solver, &rule, exclude);
                    }
                    EntityRule::Multi { targets: rules, .. } => {
                        let rules = rules
                            .iter()
                            .map(|r| self.conflict(name, &r.0))
                            .collect::<Vec<_>>();

                        let rule = z3::ast::Bool::and(&self.ctx, &rules.iter().collect::<Vec<_>>());
                        self.track(&solver, &rule, exclude);
                    }
                }
            }
        }

        let ret: HashMap<String, Vec<EntityRule>> = map
            .names
            .iter()
            .filter_map(|name| {
                let vars = RefCell::borrow_mut(&self.vars);
                let var = match vars.get(name) {
                    Some(var) => var,
                    None => {
                        warn!("No constraint for {}, skipping...", name);
                        return None;
                    }
                };

                solver.push();

                // start solving SAT of application
                solver.assert(var);

                debug!("Considering {}: {:?}", name, solver.to_string());

                // if we have envs, we need to assert them
                let envs = RefCell::borrow(&self.envs);
                let result = match envs.as_ref() {
                    Some(envs) => {
                        let mut results = HashSet::new();

                        for env in envs {
                            debug!("Cosidering env: {:?}", env.name);

                            solver.push();

                            let labels = &env.labels;
                            for label in labels {
                                if map.self_conflicts.contains(label) {
                                    let var1 = vars.get(format!("{}_1", label).as_str());
                                    let var2 = vars.get(format!("{}_2", label).as_str());

                                    match (var1, var2) {
                                        (Some(var1), Some(var2)) => {
                                            solver.assert(var1);
                                            solver.assert(var2);
                                        }
                                        _ => {
                                            warn!("No variable for {}, skipping...", label);
                                        }
                                    }
                                } else if let Some(var) = vars.get(label) {
                                    solver.assert(var);
                                } else {
                                    warn!("No variable for {}, skipping...", label);
                                }
                            }

                            for label in &map.names {
                                if labels.contains(label) || name == label {
                                    continue;
                                }

                                let var = vars.get(label).unwrap();
                                solver.assert(&var.not());
                            }

                            let result = self.check_and_get(&mut solver);
                            match result {
                                Some(r) => results.extend(r),
                                None => return None,
                            }

                            solver.pop(1u32);
                        }

                        if results.is_empty() {
                            return None;
                        }

                        Some(results.into_iter().collect::<Vec<_>>())
                    }
                    None => self.check_and_get(&mut solver),
                };

                solver.pop(1u32);

                match result {
                    Some(result) => Some((name.to_string(), result)),
                    None => None,
                }
            })
            .collect::<HashMap<_, _>>()
            .into_iter()
            .map(|(name, rules)| {
                let name = if name.contains("_") {
                    name.split("_").next().unwrap().to_string()
                } else {
                    name
                };

                (name, rules)
            })
            .fold(HashMap::new(), |mut acc, (name, rules)| {
                if let Some(existing) = acc.get_mut(&name) {
                    let merged = existing
                        .iter()
                        .chain(rules.iter())
                        .cloned()
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .collect::<Vec<_>>();

                    acc.insert(name, merged);
                } else {
                    acc.insert(name, rules);
                }

                acc
            });

        match ret.len() {
            0 => SolverOutput::Ok,
            _ => SolverOutput::Conflict(ret),
        }
    }

    fn set_envs(&'ctx self, envs: Vec<Env>) {
        debug!("using envs");

        let mut old_envs = self.envs.borrow_mut();
        old_envs.replace(envs);
    }
}
