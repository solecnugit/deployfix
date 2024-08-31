use graph_cycles::Cycles;
use std::collections::{HashMap, HashSet};

use crate::model::{Entity, EntityRule};

use super::{map::EntityMap, solver::Solver, SolverOutput};
use petgraph::{
    graph::NodeIndex,
    visit::{EdgeRef, NodeRef},
    Graph,
};

pub struct RingSolver;

impl RingSolver {
    pub fn new() -> Self {
        Self
    }

    fn get_or_create_node(
        name: &str,
        graph: &mut Graph<String, EntityRule>,
        nodes: &mut HashMap<String, NodeIndex>,
    ) -> NodeIndex {
        if let Some(node) = nodes.get(name) {
            *node
        } else {
            let node = graph.add_node(name.to_string());
            nodes.insert(name.to_string(), node);
            node
        }
    }

    fn build_graph(map: &EntityMap) -> (Graph<String, EntityRule>, HashMap<String, NodeIndex>) {
        let mut graph = Graph::new();
        let mut nodes = HashMap::<String, NodeIndex>::new();

        for entity in map.entities.iter() {
            let name = entity.name.0.as_str();
            let node = Self::get_or_create_node(name, &mut graph, &mut nodes);

            for rule in entity.requires.iter() {
                match rule {
                    EntityRule::Mono { target, .. } => {
                        let target_node =
                            Self::get_or_create_node(&target.0, &mut graph, &mut nodes);
                        graph.add_edge(node, target_node, rule.clone());
                    }
                    EntityRule::Multi { targets, .. } => {
                        for target in targets {
                            let target_node =
                                Self::get_or_create_node(&target.0, &mut graph, &mut nodes);
                            graph.add_edge(node, target_node, rule.clone());
                        }
                    }
                }
            }
        }

        (graph, nodes)
    }
}

impl Solver<'_> for RingSolver {
    fn solve(&self, entities: &EntityMap) -> SolverOutput {
        let (graph, nodes) = Self::build_graph(entities);

        let cycles = graph.cycles();
        if cycles.is_empty() {
            return SolverOutput::Ok;
        }
        let cycles = cycles
            .into_iter()
            .map(|e| e.into_iter().collect::<HashSet<_>>())
            .collect::<Vec<_>>();

        let mut conflicts = HashMap::new();
        let mut rule_ways: HashMap<EntityRule, HashSet<String>> = HashMap::new();

        for cycle in &cycles {
            if cycle.len() == 1 {
                continue;
            }

            for source_node_index in cycle {
                let source_name = graph.node_weight(*source_node_index).unwrap();
                let edges = graph.edges(*source_node_index);
                for edge in edges {
                    if edge.source() != *source_node_index {
                        continue;
                    }

                    let target_name = graph.node_weight(edge.target()).unwrap();
                    let target_node_index = nodes.get(target_name).unwrap();

                    if target_node_index == source_node_index || !cycle.contains(target_node_index)
                    {
                        continue;
                    }

                    let rule = edge.weight();
                    if source_name != rule.source().0.as_str() || !rule.is_in_target(target_name) {
                        continue;
                    }

                    let target_len = rule.targets().len();

                    if target_len > 1 {
                        let set = rule_ways.entry(rule.clone()).or_default();
                        set.insert(target_name.clone());

                        let count = set.len();

                        if count >= target_len {
                            conflicts
                                .entry(source_name.clone())
                                .or_insert_with(Vec::new)
                                .push((target_name.clone(), rule.clone()));
                        }
                    } else {
                        conflicts
                            .entry(source_name.clone())
                            .or_insert_with(Vec::new)
                            .push((target_name.clone(), rule.clone()));
                    }
                }
            }
        }

        let unsat_depends = conflicts
            .values()
            .into_iter()
            .flatten()
            .map(|(name, _)| name.clone())
            .collect::<HashSet<_>>();

        let real_conflicts = unsat_depends
            .iter()
            .filter(|name| conflicts.contains_key(*name))
            .cloned()
            .collect::<HashSet<_>>();

        let conflicts: HashMap<String, Vec<EntityRule>> = conflicts
            .into_iter()
            .map(|(name, rules)| {
                (
                    name,
                    rules
                        .into_iter()
                        .filter(|(target, _)| real_conflicts.contains(target))
                        .map(|(_, rule)| rule)
                        .collect::<Vec<_>>(),
                )
            })
            .filter(|(_, rules)| !rules.is_empty())
            .collect();

        if conflicts.is_empty() {
            SolverOutput::Ok
        } else {
            SolverOutput::Conflict(conflicts)
        }
    }

    fn set_envs(&self, envs: Vec<crate::model::Env>) {
        unreachable!()
    }
}
