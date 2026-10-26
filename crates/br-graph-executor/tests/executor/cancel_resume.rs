use std::time::Duration;

use br_graph_contract::CancelToken;
use tokio::time::Instant;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;

fn triggered_token() -> CancelToken {
    let token = CancelToken::new();
    token.handle().trigger();
    token
}

#[tokio::test(start_paused = true)]
async fn given_a_token_already_triggered_when_a_run_starts_then_it_is_cancelled_at_once_with_the_started_nodes()
 {
    let rig = chain("1", &["a", "b"]);
    let setup = Setup::new()
        .grace(Duration::from_secs(3600))
        .token(triggered_token());
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
async fn given_a_token_already_triggered_when_a_run_resumes_then_it_returns_the_starting_checkpoint()
 {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[("k", 3)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    a.failed("stop");
    let (_, checkpoint) = failed(host.outcome().await);
    let again = begin_resume(
        &rig,
        checkpoint.clone(),
        BIG,
        Setup::new().token(triggered_token()),
    );
    let (failures, returned) = cancelled(again.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(returned, checkpoint);
    assert_eq!(rig.stage.create_calls("a"), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_paused_run_when_resumed_then_the_dropped_node_starts_from_nothing_with_the_checkpoint_state()
 {
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
async fn given_a_checkpoint_later_than_the_input_a_node_saw_when_resumed_then_the_node_gets_the_later_state()
 {
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
async fn given_a_cancelled_and_a_failed_run_when_resumed_then_each_checkpoint_is_taken_up() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    quiesce().await;
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

#[tokio::test(start_paused = true)]
async fn given_an_unbounded_grace_when_nothing_is_live_then_it_does_not_overflow_and_ends_early() {
    let rig = single("a");
    let host = begin(&rig, state(&[]), &["a"], Setup::new().grace(Duration::MAX));
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    quiesce().await;
    a.cancelled();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_an_unbounded_grace_when_a_year_passes_after_the_cancel_then_a_later_result_is_applied()
 {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().grace(Duration::MAX));
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    tokio::time::advance(Duration::from_secs(365 * 24 * 3600)).await;
    quiesce().await;
    assert!(!a.is_dropped().await);
    a.result(set("k", 1));
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
    assert_eq!(checkpoint.nodes(), ["b"]);
}

#[tokio::test(start_paused = true)]
async fn given_the_grace_ended_when_a_result_is_sent_then_the_checkpoint_does_not_hold_it() {
    let rig = single("a");
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    host.cancel.trigger();
    tokio::time::advance(GRACE).await;
    let (_, checkpoint) = cancelled(host.outcome().await);
    assert!(a.is_dropped().await);
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1)]));
}
