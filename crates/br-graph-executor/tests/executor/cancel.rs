use std::time::Duration;

use tokio::time::Instant;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
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
async fn given_a_live_instance_when_the_host_cancels_then_the_run_ends_after_the_grace_with_a_checkpoint()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    quiesce().await;
    assert!(!host.is_finished());
    tokio::time::advance(Duration::from_millis(9_900)).await;
    quiesce().await;
    assert!(!host.is_finished());
    assert!(!a.is_dropped().await);
    tokio::time::advance(Duration::from_millis(100)).await;
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert!(a.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
}

#[tokio::test(start_paused = true)]
async fn given_an_instance_ignoring_the_token_when_the_grace_ends_then_it_is_dropped_without_waiting()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    tokio::time::advance(GRACE).await;
    cancelled(host.outcome().await);
    assert!(a.is_dropped().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_request_when_the_instance_answers_cancelled_then_it_is_no_failure_and_the_node_stays_not_idle()
 {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    quiesce().await;
    a.cancelled();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert!(a.is_dropped().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_when_a_result_comes_in_the_grace_then_it_is_applied_and_its_targets_are_held()
 {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    quiesce().await;
    let handed_before = count(&host.observer.log(), handed("b"));
    a.result(set("k", 1));
    let observer = host.observer.clone();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
    assert_eq!(checkpoint.nodes(), ["b"]);
    assert_eq!(rig.stage.create_calls("b"), 0);
    assert_eq!(sink.calls(), 0);
    let log = observer.log();
    assert_eq!(count(&log, edge_evaluated("a")), 1);
    assert_eq!(count(&log, handed("b")), handed_before);
}

#[tokio::test(start_paused = true)]
async fn given_the_token_seen_when_later_results_arrive_then_no_input_is_given_in_any_step() {
    let rig = rig("1")
        .entry("p")
        .entry("q")
        .node("p")
        .node("q")
        .node("x")
        .node("y")
        .edge(edge("p", &["x"]))
        .edge(edge("q", &["y"]))
        .edge(edge("x", &["END"]))
        .edge(edge("y", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["p", "q"], Setup::new());
    let mut p = rig.stage.instance("p").await;
    let mut q = rig.stage.instance("q").await;
    host.cancel.trigger();
    quiesce().await;
    p.result(set("p", 1));
    host.observer.wait_for(edge_evaluated("p")).await;
    q.result(set("q", 1));
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["x", "y"]);
    assert_eq!(rig.stage.create_calls("x") + rig.stage.create_calls("y"), 0);
    assert_eq!(state_of(&checkpoint), kv(&[("p", 1), ("q", 1)]));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_when_a_result_leaves_every_node_idle_then_the_run_is_ended() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    quiesce().await;
    a.result(set("done", 1));
    assert_eq!(value_of(&ended(host.outcome().await), "done"), Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_when_no_instance_is_live_then_the_grace_ends_early() {
    let rig = two_independent();
    let host = begin(
        &rig,
        state(&[]),
        &["a", "b"],
        Setup::new().grace(Duration::from_secs(3600)),
    );
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    let started = Instant::now();
    host.cancel.trigger();
    quiesce().await;
    a.cancelled();
    b.cancelled();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_no_instance_is_live_then_the_grace_ends_early() {
    let rig = two_independent();
    let host = begin(
        &rig,
        state(&[]),
        &["a", "b"],
        Setup::new().grace(Duration::from_secs(3600)),
    );
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    let started = Instant::now();
    a.failed("boom");
    host.observer.wait_for(handled("a")).await;
    b.cancelled();
    failed(host.outcome().await);
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[tokio::test(start_paused = true)]
async fn given_a_zero_grace_when_the_host_cancels_then_the_live_instances_are_dropped_at_once() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new().grace(Duration::ZERO));
    let mut a = rig.stage.instance("a").await;
    let started = Instant::now();
    host.cancel.trigger();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert!(a.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert!(started.elapsed() < Duration::from_millis(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_zero_grace_when_a_node_fails_then_the_other_instances_are_dropped_at_once() {
    let rig = two_independent();
    let host = begin(
        &rig,
        state(&[]),
        &["a", "b"],
        Setup::new().grace(Duration::ZERO),
    );
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.failed("boom");
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert!(b.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
}
