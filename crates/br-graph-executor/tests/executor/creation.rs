use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;
use crate::observer::*;
use crate::sink::SinkBehavior;

#[tokio::test(start_paused = true)]
async fn given_a_slow_creation_when_it_ends_then_the_first_input_is_old_and_the_held_one_is_current()
 {
    let rig = x_and_y_into_b();
    let gate = Gate::default();
    rig.stage.script("b", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["x", "y"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    let mut y = rig.stage.instance("y").await;
    x.result(set("k", 1));
    host.observer.wait_for(edge_evaluated("x")).await;
    y.result(set("k", 2));
    host.observer.wait_for(edge_evaluated("y")).await;
    assert_eq!(rig.stage.create_calls("b"), 1);
    assert!(!host.observer.log().iter().any(created("b")));
    gate.release();
    let mut b = rig.stage.instance("b").await;
    assert_eq!(b.next_input().await, kv(&[("k", 1)]));
    assert_eq!(b.next_input().await, kv(&[("k", 2)]));
    host.observer.wait_for_count(handed("b"), 2).await;
    b.result(Change::Empty);
    let observer = host.observer.clone();
    ended(host.outcome().await);
    let log = observer.log();
    assert_eq!(rig.stage.create_calls("b"), 1);
    assert_eq!(count(&log, created("b")), 1);
    assert_eq!(count(&log, handed("b")), 2);
    assert!(position(&log, created("b")) < position(&log, handed("b")));
}

#[tokio::test(start_paused = true)]
async fn given_a_slow_creation_when_other_events_arrive_then_they_are_handled_meanwhile() {
    let rig = x_and_y_into_b();
    let gate = Gate::default();
    rig.stage.script("b", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["x", "y"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    let mut y = rig.stage.instance("y").await;
    x.result(Change::Empty);
    host.observer.wait_for(edge_evaluated("x")).await;
    y.result(set("y", 1));
    host.observer.wait_for(update_applied("y")).await;
    host.observer.wait_for(edge_evaluated("y")).await;
    assert!(!host.observer.log().iter().any(created("b")));
    gate.release();
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_slow_creation_when_it_is_pending_then_no_observation_is_reported() {
    let rig = single("a");
    let gate = Gate::default();
    rig.stage.script("a", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    quiesce().await;
    assert!(host.observer.log().is_empty());
    gate.release();
    let mut a = rig.stage.instance("a").await;
    host.observer.wait_for(handed("a")).await;
    let log = host.observer.log();
    assert!(position(&log, created("a")) < position(&log, handed("a")));
    a.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_step_blocked_in_the_sink_when_a_creation_ends_then_it_is_handled_after_the_step() {
    let rig = rig("1")
        .entry("x")
        .entry("b")
        .node("x")
        .node("b")
        .node("w")
        .edge(edge("x", &["w"]))
        .edge(edge("w", &["END"]))
        .edge(edge("b", &["END"]))
        .build();
    let gate = Gate::default();
    rig.stage.script("b", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["x", "b"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let release_sink = std::sync::Arc::new(tokio::sync::Notify::new());
    sink.script(SinkBehavior::Block(release_sink.clone()));
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    sink.wait_for_calls(1).await;
    gate.release();
    quiesce().await;
    assert!(!host.observer.log().iter().any(created("b")));
    release_sink.notify_one();
    host.observer.wait_for(created("b")).await;
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    let mut w = rig.stage.instance("w").await;
    w.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_an_instance_with_another_id_when_created_then_the_run_fails() {
    let rig = single("a");
    rig.stage.script("a", Creation::WrongId);
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    let (failures, checkpoint) = failed(host.outcome().await);
    assert!(matches!(
        &failures[0],
        RunFailure::NodeCouldNotStart { node, cause: None, .. } if node == "a"
    ));
    assert_eq!(checkpoint.nodes(), ["a"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_creation_error_when_created_then_the_run_fails_with_its_cause_and_the_input_stays_held()
 {
    let rig = single("a");
    rig.stage.script("a", Creation::Fail("no model"));
    let host = begin(&rig, state(&[("k", 4)]), &["a"], Setup::new());
    let (failures, checkpoint) = failed(host.outcome().await);
    match &failures[0] {
        RunFailure::NodeCouldNotStart {
            node,
            reason,
            cause: Some(cause),
        } => {
            assert_eq!(node, "a");
            assert_eq!(reason, "no model");
            assert_eq!(cause.downcast_ref::<TestError>().unwrap().0, "no model");
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(checkpoint.nodes(), ["a"]);
    let resumed = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut a = rig.stage.instance("a").await;
    assert_eq!(a.accepted_inputs(), vec![kv(&[("k", 4)])]);
    a.result(Change::Empty);
    ended(resumed.outcome().await);
    assert_eq!(rig.stage.create_calls("a"), 2);
}

async fn creation_panic(creation: Creation) -> RunFailure {
    let rig = single("a");
    rig.stage.script("a", creation);
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    failed(host.outcome().await).0.remove(0)
}

#[tokio::test(start_paused = true)]
async fn given_a_creation_panicking_with_a_text_when_created_then_the_failure_carries_the_text() {
    for creation in [
        Creation::PanicText("model missing"),
        Creation::PanicString("model missing".to_owned()),
    ] {
        match creation_panic(creation).await {
            RunFailure::NodeCouldNotStart {
                node,
                reason,
                cause: None,
            } => {
                assert_eq!(node, "a");
                assert_eq!(reason, "model missing");
            }
            other => panic!("unexpected failure {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_creation_panicking_without_a_text_when_created_then_it_is_still_a_creation_failure()
 {
    match creation_panic(Creation::PanicOther).await {
        RunFailure::NodeCouldNotStart { reason, .. } => assert!(!reason.is_empty()),
        other => panic!("unexpected failure {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_when_a_creation_ends_then_it_keeps_its_first_input_and_is_dropped_at_the_grace_end()
 {
    let rig = x_and_y_into_b();
    let gate = Gate::default();
    rig.stage.script("b", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["x", "y"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    let mut y = rig.stage.instance("y").await;
    x.result(set("k", 1));
    host.observer.wait_for(edge_evaluated("x")).await;
    y.result(set("k", 2));
    host.observer.wait_for(edge_evaluated("y")).await;
    host.cancel.trigger();
    quiesce().await;
    gate.release();
    let mut b = rig.stage.instance("b").await;
    host.observer.wait_for(handed("b")).await;
    quiesce().await;
    assert_eq!(b.accepted_inputs(), vec![kv(&[("k", 1)])]);
    assert!(!b.is_dropped().await);
    tokio::time::advance(GRACE).await;
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert!(b.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["b"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_creation_ending_during_a_failure_when_handled_then_the_instance_is_dropped_at_the_grace_end()
 {
    let rig = rig("1")
        .entry("x")
        .entry("z")
        .node("x")
        .node("z")
        .node("b")
        .edge(edge("x", &["b"]))
        .edge(edge("z", &["END"]))
        .edge(edge("b", &["END"]))
        .build();
    let gate = Gate::default();
    rig.stage.script("b", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["x", "z"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    let mut z = rig.stage.instance("z").await;
    x.result(Change::Empty);
    host.observer.wait_for(edge_evaluated("x")).await;
    z.failed("boom");
    host.observer.wait_for(handled("z")).await;
    gate.release();
    let mut b = rig.stage.instance("b").await;
    host.observer.wait_for(handed("b")).await;
    assert!(!b.is_dropped().await);
    tokio::time::advance(GRACE).await;
    let (failures, checkpoint) = failed(host.outcome().await);
    assert_eq!(failures.len(), 1);
    assert!(b.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["b", "z"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_creation_in_progress_when_the_grace_ends_then_the_creation_is_dropped() {
    let rig = single("a");
    let flag = DropFlag::default();
    rig.stage.script("a", Creation::Never(flag.clone()));
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    quiesce().await;
    assert!(!flag.is_set());
    host.cancel.trigger();
    quiesce().await;
    assert!(!flag.is_set());
    tokio::time::advance(GRACE).await;
    let (failures, checkpoint) = cancelled(host.outcome().await);
    quiesce().await;
    assert!(flag.is_set());
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a"]);
}
