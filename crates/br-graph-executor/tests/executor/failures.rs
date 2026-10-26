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
async fn a_failed_instance_fails_the_run_with_its_cause_and_stays_not_idle() {
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
async fn a_refused_input_fails_the_run_with_its_reason() {
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
async fn a_vanished_node_fails_the_run_and_stays_not_idle() {
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
async fn a_panic_in_the_work_of_an_instance_is_seen_as_a_vanished_node() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.panic_in_task();
    let (failures, _) = failed(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeVanished { node, .. } if node == "a"));
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_event_without_a_cancel_request_counts_as_a_failure() {
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
async fn a_cancelled_event_after_the_executor_triggered_the_token_is_not_a_failure() {
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
async fn the_first_failure_triggers_the_token_for_the_host_and_every_instance() {
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
async fn a_host_trigger_reaches_every_instance() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let a = rig.stage.instance("a").await;
    let b = rig.stage.instance("b").await;
    host.cancel.trigger();
    settle().await;
    assert!(a.token.is_triggered());
    assert!(b.token.is_triggered());
    tokio::time::advance(GRACE).await;
    cancelled(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn after_a_failure_the_targets_of_a_late_result_are_held_and_its_update_is_kept() {
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
async fn a_live_instance_gets_the_whole_grace_after_a_failure_and_is_then_dropped() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.failed("boom");
    host.observer.wait_for(handled("a")).await;
    tokio::time::advance(Duration::from_millis(9_900)).await;
    settle().await;
    assert!(!b.is_dropped().await);
    assert!(!host.is_finished());
    tokio::time::advance(Duration::from_millis(100)).await;
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert!(b.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
}

#[tokio::test(start_paused = true)]
async fn a_failure_after_the_host_triggered_the_token_leaves_the_outcome_a_cancel() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    settle().await;
    a.failed("late");
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeFailed { node, .. } if node == "a"));
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn a_failure_before_the_host_triggers_the_token_wins_over_the_cancel() {
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
async fn failures_are_listed_in_the_order_they_were_handled_and_the_first_is_the_cause() {
    for (first, second) in [("a", "b"), ("b", "a")] {
        let rig = two_independent();
        let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
        let mut a = rig.stage.instance("a").await;
        let mut b = rig.stage.instance("b").await;
        let (mut one, mut two) = if first == "a" { (a, b) } else { (b, a) };
        one.failed("first");
        host.observer
            .wait_for(if first == "a" { handled("a") } else { handled("b") })
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
async fn the_checkpoint_of_a_failure_holds_the_state_after_every_finished_step() {
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
async fn an_instance_dropped_at_the_end_of_the_grace_counts_as_not_idle_and_is_no_fault() {
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

async fn failing_run(kind: &str) -> (Rig, br_graph_executor::Checkpoint) {
    let rig = match kind {
        "edge" => rig("1")
            .entry("a")
            .node("a")
            .edge(edge_failing("a", &["END"], "edge broke"))
            .build(),
        _ => single("a"),
    };
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    match kind {
        "update" => a.result(Change::Refuse),
        "edge" => a.result(set("k", 9)),
        _ => a.refused("not twice"),
    }
    let (_, checkpoint) = failed(host.outcome().await);
    (rig, checkpoint)
}

#[tokio::test(start_paused = true)]
async fn a_resume_after_a_refused_update_an_edge_failure_or_a_refused_input_runs_the_node_again() {
    for kind in ["update", "edge", "refused"] {
        let (rig, checkpoint) = failing_run(kind).await;
        let host = begin_resume(&rig, checkpoint, BIG, Setup::new());
        let mut again = rig.stage.instance("a").await;
        assert_eq!(again.accepted_inputs(), vec![kv(&[("k", 1)])]);
        again.failed("stop");
        failed(host.outcome().await);
        assert_eq!(rig.stage.create_calls("a"), 2);
    }
}

#[tokio::test(start_paused = true)]
async fn a_node_that_refuses_its_input_refuses_it_again_after_a_resume() {
    let (rig, checkpoint) = failing_run("refused").await;
    let host = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut again = rig.stage.instance("a").await;
    again.refused("not twice");
    let (failures, _) = failed(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeRefusedInput { .. }));
}
