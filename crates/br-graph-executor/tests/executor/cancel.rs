use std::time::Duration;

use br_graph_contract::CancelToken;
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
async fn a_cancel_needs_no_event_and_ends_after_the_grace_with_a_checkpoint() {
    let rig = single("a");
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    settle().await;
    assert!(!host.is_finished());
    tokio::time::advance(Duration::from_millis(9_900)).await;
    settle().await;
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
async fn an_instance_that_ignores_the_token_is_dropped_without_the_run_waiting_for_its_work() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    tokio::time::advance(GRACE).await;
    cancelled(host.outcome().await);
    assert!(a.is_dropped().await);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_event_after_the_cancel_request_is_normal_and_leaves_the_node_not_idle() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    settle().await;
    a.cancelled();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert!(a.is_dropped().await);
}

#[tokio::test(start_paused = true)]
async fn a_result_in_the_grace_is_applied_its_edge_evaluated_and_its_targets_held() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    settle().await;
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
async fn once_the_token_is_seen_no_input_is_given_in_any_later_step() {
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
    settle().await;
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
async fn a_result_that_leaves_every_node_idle_ends_the_run_whatever_the_token() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    settle().await;
    a.result(set("done", 1));
    assert_eq!(value_of(&ended(host.outcome().await), "done"), Some(1));
}

#[tokio::test(start_paused = true)]
async fn the_grace_ends_early_when_no_instance_is_live_after_a_cancel() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new().grace(Duration::from_secs(3600)));
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    let started = Instant::now();
    host.cancel.trigger();
    settle().await;
    a.cancelled();
    b.cancelled();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
    assert!(started.elapsed() < Duration::from_secs(60));
}

#[tokio::test(start_paused = true)]
async fn the_grace_ends_early_when_no_instance_is_live_after_a_failure() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new().grace(Duration::from_secs(3600)));
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
async fn a_zero_grace_drops_the_live_instances_at_once() {
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
async fn a_zero_grace_after_a_failure_drops_the_other_instances_at_once() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new().grace(Duration::ZERO));
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.failed("boom");
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert!(b.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
}

fn triggered_token() -> CancelToken {
    let token = CancelToken::new();
    token.handle().trigger();
    token
}

#[tokio::test(start_paused = true)]
async fn a_token_already_triggered_gives_a_cancel_at_once_with_the_state_and_the_started_nodes() {
    let rig = chain("1", &["a", "b"]);
    let setup = Setup::new().grace(Duration::from_secs(3600)).token(triggered_token());
    let started = Instant::now();
    let host = begin(&rig, state(&[("k", 3)]), &["b", "a"], setup);
    let observer = host.observer.clone();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(state_of(&checkpoint), kv(&[("k", 3)]));
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
    assert_eq!(rig.stage.create_calls("a") + rig.stage.create_calls("b"), 0);
    assert!(started.elapsed() < Duration::from_secs(1));
    let log = observer.wait_for_outcome().await;
    assert_eq!(log.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_token_already_triggered_on_a_resume_returns_the_checkpoint_it_started_from() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[("k", 3)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("stop");
    let (_, checkpoint) = failed(host.outcome().await);
    let again = begin_resume(&rig, checkpoint.clone(), BIG, Setup::new().token(triggered_token()));
    let (failures, returned) = cancelled(again.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(returned, checkpoint);
    assert_eq!(rig.stage.create_calls("a"), 1);
}

#[tokio::test(start_paused = true)]
async fn a_paused_run_resumes_from_its_checkpoint_and_a_dropped_node_starts_from_nothing() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let mut b = rig.stage.instance("b").await;
    assert_eq!(b.accepted_inputs(), vec![kv(&[("k", 1)])]);
    host.cancel.trigger();
    tokio::time::advance(GRACE).await;
    let (_, checkpoint) = cancelled(host.outcome().await);
    assert_eq!(checkpoint.nodes(), ["b"]);
    let resumed = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut b_again = rig.stage.instance("b").await;
    assert_ne!(b_again.id, b.id);
    assert_eq!(b_again.accepted_inputs(), vec![kv(&[("k", 1)])]);
    b_again.result(set("done", 1));
    assert_eq!(value_of(&ended(resumed.outcome().await), "done"), Some(1));
}

#[tokio::test(start_paused = true)]
async fn a_resumed_node_receives_the_state_of_the_checkpoint_which_can_be_later_than_the_one_it_saw() {
    let rig = rig("1")
        .entry("a")
        .entry("c")
        .node("a")
        .node("b")
        .node("c")
        .edge(edge("a", &["b"]))
        .edge(edge("b", &["END"]))
        .edge(edge("c", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "c"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut c = rig.stage.instance("c").await;
    a.result(set("k", 1));
    let mut b = rig.stage.instance("b").await;
    assert_eq!(b.accepted_inputs(), vec![kv(&[("k", 1)])]);
    c.result(set("k", 2));
    host.observer.wait_for(edge_evaluated("c")).await;
    host.cancel.trigger();
    tokio::time::advance(GRACE).await;
    let (_, checkpoint) = cancelled(host.outcome().await);
    let resumed = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut again = rig.stage.instance("b").await;
    assert_eq!(again.accepted_inputs(), vec![kv(&[("k", 2)])]);
    again.result(Change::Empty);
    ended(resumed.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_and_a_failed_run_can_both_be_taken_up_by_a_resume() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    settle().await;
    a.cancelled();
    let (_, from_cancel) = cancelled(host.outcome().await);
    let resumed = begin_resume(&rig, from_cancel, BIG, Setup::new());
    let mut a_again = rig.stage.instance("a").await;
    a_again.failed("boom");
    let (_, from_failure) = failed(resumed.outcome().await);
    let third = begin_resume(&rig, from_failure, BIG, Setup::new());
    let mut a_third = rig.stage.instance("a").await;
    a_third.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    ended(third.outcome().await);
}
