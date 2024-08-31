use std::collections::BTreeSet;

use deployfix::{
    model::{Entity, EntityName, EntityRule, EntityRuleSource, EntityRuleType, EntitySource},
    solver::{self, get_solver, SolverOutput},
};
use either::Either;
use log::debug;

// Init
#[cfg(test)]
#[ctor::ctor]
fn init() {
    flexi_logger::Logger::try_with_env()
        .expect("Failed to initialize logger")
        .start()
        .expect("Failed to initialize logger");
}

fn solve(entities: Vec<Entity>) -> bool {
    let entity_map = entities.try_into().unwrap();

    debug!("Entity map: {:?}", entity_map);

    let solver = get_solver("z3").unwrap();
    let z3_result = solver.solve(&entity_map);

    let ring_solver = get_solver("ring").unwrap();
    let ring_result = ring_solver.solve(&entity_map);

    let result = ring_result.merge(z3_result);

    match result {
        SolverOutput::Ok => {
            debug!("No conflict found");
            true
        }
        SolverOutput::Conflict(conflicts) => {
            debug!("Conflicts found: {:?}", conflicts);
            false
        }
    }
}

fn new_with_mono_rules(name: &str, requires: Vec<&str>, excludes: Vec<&str>) -> Entity {
    let source = EntityName(name.to_string());

    Entity {
        name: name.into(),
        requires: requires
            .into_iter()
            .map(|s| {
                EntityRule::mono(
                    source.clone(),
                    s.to_string().into(),
                    EntityRuleType::Require,
                    EntityRuleSource::Unknown,
                    None,
                )
            })
            .collect(),
        excludes: excludes
            .into_iter()
            .map(|s| {
                EntityRule::mono(
                    source.clone(),
                    s.to_string().into(),
                    EntityRuleType::Exclude,
                    EntityRuleSource::Unknown,
                    None,
                )
            })
            .collect(),
        source: EntitySource::Unknown,
        priority: deployfix::model::EntityPriority::default(),
    }
}

fn new_with_either_rules(
    name: &str,
    requires: Vec<Either<&str, Vec<&str>>>,
    excludes: Vec<Either<&str, Vec<&str>>>,
) -> Entity {
    let source = EntityName(name.to_string());

    Entity {
        name: name.into(),
        requires: requires
            .into_iter()
            .map(|e| match e {
                Either::Left(s) => EntityRule::mono(
                    source.clone(),
                    s.to_string().into(),
                    EntityRuleType::Require,
                    EntityRuleSource::Unknown,
                    None,
                ),
                Either::Right(v) => EntityRule::multi(
                    source.clone(),
                    v.into_iter()
                        .map(|s| s.to_string().into())
                        .collect::<BTreeSet<_>>(),
                    EntityRuleType::Require,
                    EntityRuleSource::Unknown,
                    None,
                ),
            })
            .collect(),
        excludes: excludes
            .into_iter()
            .map(|e| match e {
                Either::Left(s) => EntityRule::mono(
                    source.clone(),
                    s.to_string().into(),
                    EntityRuleType::Exclude,
                    EntityRuleSource::Unknown,
                    None,
                ),
                Either::Right(v) => EntityRule::multi(
                    source.clone(),
                    v.into_iter()
                        .map(|s| s.to_string().into())
                        .collect::<BTreeSet<_>>(),
                    EntityRuleType::Exclude,
                    EntityRuleSource::Unknown,
                    None,
                ),
            })
            .collect(),
        source: EntitySource::Unknown,
        priority: deployfix::model::EntityPriority::default(),
    }
}

/*
    pod require node
    Expected: satisfiable
*/
#[test]
fn test_singleton_affinity() {
    let entities = vec![
        new_with_mono_rules("pod", vec!["node"], vec![]),
        new_with_mono_rules("node", vec![], vec![]),
    ];

    assert!(solve(entities));
}

/*
    pod exclude node
    Expected: satisfiable
*/
#[test]
fn test_singleton_anti_affinity() {
    let entities = vec![
        new_with_mono_rules("pod", vec![], vec!["node"]),
        new_with_mono_rules("node", vec![], vec![]),
    ];

    assert!(solve(entities));
}

/*
    pod require pod
    Expected: satisfiable
*/
#[test]
fn test_singleton_self_affinity() {
    let entities = vec![new_with_mono_rules("pod", vec!["pod"], vec![])];

    assert!(solve(entities));
}

/*
    pod exclude pod
    Expected: unsatisfiable
*/
#[test]
fn test_singleton_self_anti_affinity() {
    let entities = vec![new_with_mono_rules("pod", vec![], vec!["pod"])];

    assert!(solve(entities));
}

/*
    pod require pod
    pod exclude pod
    Expected: unsatisfiable
*/
#[test]
fn test_singleton_self_affinity_and_anti_affinity() {
    let entities = vec![new_with_mono_rules("pod", vec!["pod"], vec!["pod"])];

    assert!(!solve(entities));
}

/*
    pod require node
    node require rack
    Expected: satisfiable
*/
#[test]
fn test_transitive_affinity() {
    let entities = vec![
        new_with_mono_rules("pod", vec!["node"], vec![]),
        new_with_mono_rules("node", vec!["rack"], vec![]),
        new_with_mono_rules("rack", vec![], vec![]),
    ];

    assert!(solve(entities));
}

/*
    pod require node
    node exclude rack
    Expected: satisfiable
*/
#[test]
fn test_transitive_anti_affinity() {
    let entities = vec![
        new_with_mono_rules("pod", vec![], vec!["node"]),
        new_with_mono_rules("node", vec![], vec!["rack"]),
        new_with_mono_rules("rack", vec![], vec![]),
    ];

    assert!(solve(entities));
}

/*
    app1 require app1;app2;app3
    app1 exclude app1
    Expected: satisfiable
*/
#[test]
fn test_self_affinity_and_anti_affinity() {
    let entities = vec![new_with_either_rules(
        "app1",
        vec![Either::Right(vec!["app1", "app2", "app3"])],
        vec![Either::Left("app1")],
    )];

    assert!(solve(entities));
}

/*
    app1 require app1;app2
    app1 exclude app1;app2;app3
    Expected: unsatisfiable
*/
#[test]
fn test_self_affinity_and_anti_affinity_2() {
    let entities = vec![new_with_either_rules(
        "app1",
        vec![Either::Right(vec!["app1", "app2"])],
        vec![Either::Right(vec!["app1", "app2", "app3"])],
    )];

    assert!(!solve(entities));
}

/*
    app1 require app2;app3
    app1 exclude app2;app3;app4
    Expected: unsatisfiable
*/
#[test]
fn test_self_affinity_and_anti_affinity_3() {
    let entities = vec![new_with_either_rules(
        "app1",
        vec![Either::Right(vec!["app2", "app3"])],
        vec![Either::Right(vec!["app2", "app3", "app4"])],
    )];

    assert!(!solve(entities));
}

/*
    app1 require app2
    app2 require app1
*/
#[test]
fn test_circular_dependencies() {
    let entities = vec![
        new_with_mono_rules("app1", vec!["app2"], vec![]),
        new_with_mono_rules("app2", vec!["app1"], vec![]),
    ];

    assert!(!solve(entities));
}
