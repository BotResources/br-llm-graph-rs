use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;
use crate::observer::*;

#[tokio::test(start_paused = true)]
async fn given_a_failing_instance_when_it_sends_failed_then_the_run_fails_and_the_node_stays_not_idle()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("boom");
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    match &failures[0] {
        RunFailure::NodeFailed {
            node,
            reason,
            cause,
        } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "boom");
            let cause = cause.as_ref().unwrap();
            assert_eq!(cause.downcast_ref::<TestError>().unwrap().0, "boom");
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(failures[0].to_string(), "node \"a\" failed: boom");
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
    assert!(a.is_dropped().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_refused_input_when_the_instance_answers_then_the_run_fails_with_the_reason() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.refused("not twice");
    let (failures, checkpoint) = failed(host.outcome().await);
    match &failures[0] {
        RunFailure::NodeRefusedInput { node, reason } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "not twice");
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(
        failures[0].to_string(),
        "node \"a\" refused its input: not twice"
    );
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert!(a.is_dropped().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_vanished_node_when_detected_then_the_run_fails_and_the_node_stays_not_idle() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.vanish();
    let (failures, checkpoint) = failed(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeVanished { node, .. } if node == "a"));
    assert_eq!(
        failures[0].to_string(),
        "node \"a\" vanished: the node side ended without a terminal event"
    );
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_an_instance_whose_work_panics_when_its_event_end_closes_then_the_node_is_seen_as_vanished()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.panic_in_task();
    let (failures, _) = failed(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeVanished { node, .. } if node == "a"));
}

#[tokio::test(start_paused = true)]
async fn given_no_cancel_request_when_an_instance_answers_cancelled_then_it_counts_as_a_failure() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.cancelled();
    let (failures, checkpoint) = failed(host.outcome().await);
    match &failures[0] {
        RunFailure::NodeFailed {
            node,
            reason,
            cause,
        } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "cancelled without a cancel request");
            assert!(cause.is_none());
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_an_instance_answers_cancelled_then_it_is_no_new_failure() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    b.failed("first");
    host.observer.wait_for(handled("b")).await;
    a.cancelled();
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert!(matches!(&failures[0], RunFailure::NodeFailed { node, .. } if node == "b"));
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
}
