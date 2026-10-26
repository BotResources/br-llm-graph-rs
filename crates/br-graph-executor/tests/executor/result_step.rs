use std::num::NonZeroUsize;

use br_graph_contract::{State, Target};
use br_graph_executor::{Observation, RunFailure};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;
use crate::observer::*;

async fn undone_result(update: Change) -> (RunFailure, br_graph_executor::Checkpoint, bool) {
    let rig = single("a");
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(update);
    let (mut failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    let dropped = a.is_dropped().await;
    (failures.remove(0), checkpoint, dropped)
}

#[tokio::test(start_paused = true)]
async fn given_a_refused_update_when_handled_then_the_step_is_undone_and_the_node_stays_not_idle() {
    let (failure, checkpoint, dropped) = undone_result(Change::Refuse).await;
    assert!(matches!(failure, RunFailure::UpdateRefused { node, .. } if node == "a"));
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert!(dropped);
}

#[tokio::test(start_paused = true)]
async fn given_an_update_giving_invalid_fields_when_applied_then_it_is_refused() {
    let (failure, checkpoint, _) = undone_result(Change::Invalid).await;
    assert!(matches!(failure, RunFailure::UpdateRefused { .. }));
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
}

#[tokio::test(start_paused = true)]
async fn given_an_update_over_the_size_limit_when_applied_then_it_is_refused() {
    let rig = single("a");
    let small = State::create_with_limit(kv(&[("k", 1)]), NonZeroUsize::new(100).unwrap()).unwrap();
    let host = begin(&rig, small, &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Grow(200));
    let (failures, checkpoint) = failed(host.outcome().await);
    assert!(matches!(failures[0], RunFailure::UpdateRefused { .. }));
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
}

#[tokio::test(start_paused = true)]
async fn given_fields_that_panic_when_an_update_is_applied_then_the_update_is_refused() {
    let (failure, checkpoint, _) = undone_result(Change::Boom).await;
    match failure {
        RunFailure::UpdateRefused { reason, .. } => assert_eq!(reason, "the fields panic"),
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
}

async fn edge_failure(rig: Rig) -> (RunFailure, br_graph_executor::Checkpoint) {
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 9));
    let (mut failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    (failures.remove(0), checkpoint)
}

fn with_edge_of_a(edge: std::sync::Arc<dyn br_graph_contract::Edge<Kv>>) -> Rig {
    rig("1")
        .entry("a")
        .node("a")
        .node("b")
        .node("c")
        .edge(edge)
        .edge(crate::graphs::edge("b", &["END"]))
        .edge(crate::graphs::edge("c", &["END"]))
        .entry("c")
        .build()
}

#[tokio::test(start_paused = true)]
async fn given_an_edge_that_fails_when_evaluated_then_the_step_is_undone_and_the_cause_is_kept() {
    let rig = with_edge_of_a(edge_failing("a", &["b", "END"], "edge broke"));
    let (failure, checkpoint) = edge_failure(rig).await;
    match failure {
        RunFailure::EdgeFailed {
            node,
            cause: Some(cause),
            ..
        } => {
            assert_eq!(node, "a");
            assert_eq!(cause.downcast_ref::<TestError>().unwrap().0, "edge broke");
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_an_edge_breaking_the_checked_evaluation_when_evaluated_then_the_run_fails_without_a_cause()
 {
    let broken = [
        edge_returning("a", &["b", "END"], &["c"]),
        edge_returning("a", &["b", "END"], &[]),
        edge_returning("a", &["b", "END"], &["b", "END"]),
    ];
    for edge in broken {
        let (failure, checkpoint) = edge_failure(with_edge_of_a(edge)).await;
        assert!(matches!(
            failure,
            RunFailure::EdgeFailed { ref node, cause: None, .. } if node == "a"
        ));
        assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_edge_that_panics_when_evaluated_then_it_is_an_edge_failure_without_a_cause() {
    let rig = with_edge_of_a(edge_panicking("a", &["b", "END"]));
    let (failure, _) = edge_failure(rig).await;
    match failure {
        RunFailure::EdgeFailed {
            reason,
            cause: None,
            ..
        } => assert_eq!(reason, "the edge panics"),
        other => panic!("unexpected failure {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_target_repeated_by_an_edge_when_handed_over_then_one_input_is_given_and_reported_once()
 {
    let rig = rig("1")
        .entry("x")
        .node("x")
        .node("b")
        .edge(edge_returning("x", &["b"], &["b", "b"]))
        .edge(edge("b", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["x"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    assert_eq!(b.accepted_inputs().len(), 1);
    b.result(Change::Empty);
    let observer = host.observer.clone();
    ended(host.outcome().await);
    assert!(observer.log().contains(&Observation::EdgeEvaluated {
        node: "x".into(),
        targets: vec![Target::node("b")]
    }));
    assert_eq!(count(&observer.log(), handed("b")), 1);
}

#[tokio::test(start_paused = true)]
async fn given_results_from_two_nodes_when_handled_then_updates_are_applied_in_handling_order() {
    for (first, second, value_of_first, value_of_second) in [("a", "b", 10, 20), ("b", "a", 20, 10)]
    {
        let rig = two_independent();
        let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
        let mut one = rig.stage.instance(first).await;
        let mut two = rig.stage.instance(second).await;
        one.result(set("k", value_of_first));
        host.observer.wait_for(update_applied(first)).await;
        two.result(set("k", value_of_second));
        host.observer.wait_for(update_applied(second)).await;
        assert_eq!(
            value_of(&ended(host.outcome().await), "k"),
            Some(value_of_second)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn given_two_results_when_the_second_edge_runs_then_it_sees_every_update_applied_before() {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .node("yes")
        .node("no")
        .edge(edge("a", &["END"]))
        .edge(routing_on("b", &["yes", "no"], |fields| {
            if fields.values.get("x") == Some(&1) {
                vec!["yes"]
            } else {
                vec!["no"]
            }
        }))
        .edge(edge("yes", &["END"]))
        .edge(edge("no", &["END"]))
        .entry("yes")
        .entry("no")
        .build();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.result(set("x", 1));
    host.observer.wait_for(edge_evaluated("a")).await;
    b.result(Change::Empty);
    let mut yes = rig.stage.instance("yes").await;
    yes.result(Change::Empty);
    ended(host.outcome().await);
    assert_eq!(rig.stage.create_calls("no"), 0);
}
