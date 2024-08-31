use deployfix::{
    model::{Entity, EntityRule, EntityRuleSource, EntityRuleType},
    solver::get_solver,
};
use petgraph::visit::{EdgeRef, IntoEdges};
use rand::{rngs::SmallRng, Rng, SeedableRng};

#[cfg(test)]
#[ctor::ctor]
fn init() {
    flexi_logger::Logger::try_with_env()
        .expect("Failed to initialize logger")
        .start()
        .expect("Failed to initialize logger");
}

struct Edge {
    r#type: EntityRuleType,
}

impl Edge {
    fn new(r#type: EntityRuleType) -> Self {
        Self { r#type }
    }
}

impl petgraph::EdgeType for Edge {
    fn is_directed() -> bool {
        true
    }
}

fn random_graph(
    node_size: u32,
    edge_size: u32,
    rule_func: impl Fn(&mut SmallRng, u32, u32, &petgraph::Graph<(), Edge>) -> EntityRuleType,
) -> petgraph::Graph<(), Edge> {
    let mut rng = rand::prelude::SmallRng::seed_from_u64(0);
    let mut graph: petgraph::prelude::Graph<(), Edge> = petgraph::Graph::new();

    for _ in 0..node_size {
        graph.add_node(());
    }

    for _ in 0..edge_size {
        let a = rng.gen_range(0..node_size);
        let b = rng.gen_range(0..node_size);

        let r#type = rule_func(&mut rng, a, b, &graph);
        let (a, b) = (a.into(), b.into());

        graph.add_edge(a, b, Edge::new(r#type));
    }

    graph
}

fn graph_to_entities(graph: &petgraph::Graph<(), Edge>) -> Vec<Entity> {
    graph
        .node_indices()
        .filter_map(|node| {
            let edge_count = graph.edges(node).count();
            if edge_count == 0 {
                return None;
            }

            let name = format!("app{}", node.index());
            let entity = graph
                .edges(node)
                .fold(Entity::new(&name), |mut entity, edge| {
                    let target = format!("app{}", edge.target().index());
                    let r#type = &edge.weight().r#type;

                    match r#type {
                        EntityRuleType::Require => entity.add_require(EntityRule::mono(
                            name.clone().into(),
                            target.clone().into(),
                            r#type.clone(),
                            EntityRuleSource::Unknown,
                            None,
                        )),
                        EntityRuleType::Exclude => entity.add_exclude(EntityRule::mono(
                            name.clone().into(),
                            target.clone().into(),
                            r#type.clone(),
                            EntityRuleSource::Unknown,
                            None,
                        )),
                    }

                    entity
                });

            Some(entity)
        })
        .collect::<Vec<_>>()
}

#[test]
fn test_random_graph_with_only_require() {
    let graph = random_graph(100, 50, |_, _, _, _| EntityRuleType::Require);
    let entities = graph_to_entities(&graph);
    let entity_map = entities
        .try_into()
        .expect("failed to convert entities to entity map");

    let solver = get_solver("z3").expect("failed to get solver");
    let output = solver.solve(&entity_map);

    assert!(output.is_ok());
}

#[test]
fn test_random_graph_with_only_exclude() {
    let graph = random_graph(100, 50, |_, _, _, _| EntityRuleType::Exclude);
    let entities = graph_to_entities(&graph);
    let entity_map = entities
        .try_into()
        .expect("failed to convert entities to entity map");

    let solver = get_solver("z3").expect("failed to get solver");
    let output = solver.solve(&entity_map);

    assert!(output.is_ok());
}
