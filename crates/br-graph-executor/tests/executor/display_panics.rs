use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;

#[tokio::test(start_paused = true)]
async fn given_a_creation_error_whose_display_panics_when_the_node_is_created_then_the_run_fails_with_the_panic_text()
 {
    let rig = single("a");
    rig.stage.script(
        "a",
        Creation::FailPanickingDisplay("the creation error panics"),
    );
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    match &failures[0] {
        RunFailure::NodeCouldNotStart {
            node,
            reason,
            cause,
        } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "the creation error panics");
            let cause = cause.as_ref().unwrap();
            assert!(cause.downcast_ref::<PanickingDisplay>().is_some());
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(
        failures[0].to_string(),
        "node \"a\" could not start: the creation error panics"
    );
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_failed_event_whose_display_panics_when_it_is_handled_then_the_run_fails_with_the_panic_text()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed_panicking_display("the node cause panics");
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    match &failures[0] {
        RunFailure::NodeFailed {
            node,
            reason,
            cause,
        } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "the node cause panics");
            assert!(cause.downcast_ref::<PanickingDisplay>().is_some());
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(
        failures[0].to_string(),
        "node \"a\" failed: the node cause panics"
    );
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_an_edge_error_whose_display_panics_when_the_edge_is_evaluated_then_the_run_fails_with_the_panic_text()
 {
    let rig = rig("1")
        .entry("a")
        .node("a")
        .edge(edge_failing_panicking_display(
            "a",
            &["END"],
            "the edge cause panics",
        ))
        .build();
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    match &failures[0] {
        RunFailure::EdgeFailed {
            node,
            reason,
            cause,
        } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "the edge cause panics");
            let cause = cause.as_ref().unwrap();
            assert!(cause.downcast_ref::<PanickingDisplay>().is_some());
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(
        failures[0].to_string(),
        "edge of node \"a\" failed: the edge cause panics"
    );
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert_eq!(state_of(&checkpoint), kv(&[]));
}
