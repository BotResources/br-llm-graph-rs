use std::time::Duration;

use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;
use crate::observer::*;

fn two_independent() -> Rig {
    rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .edge(edge("a", &["END"]))
        .edge(edge("b", &["END"]))
        .build()
}

#[tokio::test(start_paused = true)]
async fn given_a_failing_instance_when_it_sends_failed_then_the_run_fails_with_its_cause_and_the_node_stays_not_idle()
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
            assert!(reason.contains("boom"));
            assert_eq!(cause.downcast_ref::<TestError>().unwrap().0, "boom");
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert!(!failures[0].to_string().is_empty());
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
            assert!(reason.contains("not twice"));
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert!(!failures[0].to_string().is_empty());
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
    assert!(!failures[0].to_string().is_empty());
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
        RunFailure::NodeFailed { node, reason, .. } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "cancelled without a cancel request");
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_that_triggered_the_token_when_an_instance_answers_cancelled_then_it_is_no_new_failure()
 {
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

#[tokio::test(start_paused = true)]
async fn given_a_first_failure_when_detected_then_the_token_is_triggered_for_the_host_and_every_instance()
 {
    let rig = rig("1")
        .entry("f")
        .entry("c")
        .entry("d")
        .node("f")
        .node("c")
        .node("d")
        .edge(edge("f", &["END"]))
        .edge(edge("c", &["END"]))
        .edge(edge("d", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["f", "c", "d"], Setup::new());
    let mut f = rig.stage.instance("f").await;
    let c = rig.stage.instance("c").await;
    let d = rig.stage.instance("d").await;
    assert!(!c.token.is_triggered());
    f.failed("boom");
    host.observer.wait_for(handled("f")).await;
    assert!(host.cancel.is_triggered());
    assert!(c.token.is_triggered());
    assert!(d.token.is_triggered());
    tokio::time::advance(GRACE).await;
    failed(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_live_instances_when_the_host_triggers_the_token_then_every_instance_sees_it() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let a = rig.stage.instance("a").await;
    let b = rig.stage.instance("b").await;
    host.cancel.trigger();
    quiesce().await;
    assert!(a.token.is_triggered());
    assert!(b.token.is_triggered());
    tokio::time::advance(GRACE).await;
    cancelled(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_a_late_result_arrives_then_its_update_is_kept_and_its_targets_are_held()
 {
    let rig = rig("1")
        .entry("f")
        .entry("a")
        .node("f")
        .node("a")
        .node("x")
        .edge(edge("f", &["END"]))
        .edge(edge("a", &["x"]))
        .edge(edge("x", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["f", "a"], Setup::new());
    let mut f = rig.stage.instance("f").await;
    let mut a = rig.stage.instance("a").await;
    f.failed("boom");
    host.observer.wait_for(handled("f")).await;
    a.result(set("k", 1));
    let observer = host.observer.clone();
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert_eq!(rig.stage.create_calls("x"), 0);
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
    assert_eq!(checkpoint.nodes(), ["f", "x"]);
    let log = observer.log();
    assert_eq!(count(&log, edge_evaluated("a")), 1);
    assert_eq!(count(&log, handed("x")), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_another_instance_stays_live_then_it_gets_the_whole_grace_and_is_then_dropped()
 {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.failed("boom");
    host.observer.wait_for(handled("a")).await;
    tokio::time::advance(Duration::from_millis(9_900)).await;
    quiesce().await;
    assert!(!b.is_dropped().await);
    assert!(!host.is_finished());
    tokio::time::advance(Duration::from_millis(100)).await;
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert!(b.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
}

#[tokio::test(start_paused = true)]
async fn given_the_host_triggered_the_token_when_a_node_fails_then_the_outcome_is_a_cancel_with_the_failure_listed()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    quiesce().await;
    a.failed("late");
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeFailed { node, .. } if node == "a"));
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_the_host_cancels_afterwards_then_the_run_is_failed() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let _b = rig.stage.instance("b").await;
    a.failed("first");
    host.observer.wait_for(handled("a")).await;
    host.cancel.trigger();
    tokio::time::advance(GRACE).await;
    failed(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_several_failures_when_the_run_stops_then_they_are_listed_in_order_and_the_first_is_the_cause()
 {
    for (first, second) in [("a", "b"), ("b", "a")] {
        let rig = two_independent();
        let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
        let a = rig.stage.instance("a").await;
        let b = rig.stage.instance("b").await;
        let (mut one, mut two) = if first == "a" { (a, b) } else { (b, a) };
        one.failed("first");
        host.observer
            .wait_for(if first == "a" {
                handled("a")
            } else {
                handled("b")
            })
            .await;
        two.refused("second");
        let (failures, _) = failed(host.outcome().await);
        assert_eq!(failures.len(), 2);
        assert!(matches!(&failures[0], RunFailure::NodeFailed { node, .. } if node == first));
        assert!(
            matches!(&failures[1], RunFailure::NodeRefusedInput { node, .. } if node == second)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_the_run_stops_then_the_checkpoint_holds_the_state_after_every_finished_step()
 {
    let rig = chain("1", &["a", "b", "c"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let mut b = rig.stage.instance("b").await;
    b.result(set("k", 2));
    let mut c = rig.stage.instance("c").await;
    c.failed("boom");
    let (_, checkpoint) = failed(host.outcome().await);
    assert_eq!(state_of(&checkpoint), kv(&[("k", 2)]));
    assert_eq!(checkpoint.nodes(), ["c"]);
}

#[tokio::test(start_paused = true)]
async fn given_an_instance_dropped_at_the_grace_end_when_the_run_stops_then_it_is_not_idle_and_no_fault()
 {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.failed("boom");
    host.observer.wait_for(handled("a")).await;
    tokio::time::advance(GRACE).await;
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
    assert!(b.is_dropped().await);
}
