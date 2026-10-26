use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;
use crate::observer::*;
use crate::sink::SinkBehavior;

fn two_sources_into_b() -> Rig {
    rig("1")
        .entry("x")
        .entry("y")
        .entry("b")
        .node("x")
        .node("y")
        .node("b")
        .edge(edge("x", &["b"]))
        .edge(edge("y", &["b"]))
        .edge(edge("b", &["END"]))
        .build()
}

#[tokio::test(start_paused = true)]
async fn a_creation_gets_the_state_of_the_step_that_started_it_and_later_the_held_input_with_the_state_at_that_moment()
 {
    let rig = two_sources_into_b();
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
async fn the_handling_of_events_goes_on_while_a_creation_is_slow() {
    let rig = two_sources_into_b();
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
async fn no_input_is_reported_before_the_creation_ends() {
    let rig = single("a");
    let gate = Gate::default();
    rig.stage.script("a", Creation::Gated(gate.clone()));
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    settle().await;
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
async fn the_end_of_a_creation_waits_for_the_step_in_progress() {
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
    settle().await;
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
async fn an_instance_with_another_id_than_the_given_one_fails_the_run() {
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
async fn a_creation_error_fails_the_run_with_its_cause_and_keeps_the_input_held() {
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
            assert!(reason.contains("no model"));
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
async fn a_creation_that_panics_with_a_text_is_a_creation_failure_carrying_the_text() {
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
                assert!(reason.contains("model missing"));
            }
            other => panic!("unexpected failure {other:?}"),
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_creation_that_panics_without_a_text_is_still_a_creation_failure() {
    match creation_panic(Creation::PanicOther).await {
        RunFailure::NodeCouldNotStart { reason, .. } => assert!(!reason.is_empty()),
        other => panic!("unexpected failure {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_creation_that_ends_after_the_cancel_keeps_its_first_input_gets_no_other_and_is_dropped_at_the_end_of_the_grace()
 {
    let rig = two_sources_into_b();
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
    settle().await;
    gate.release();
    let mut b = rig.stage.instance("b").await;
    host.observer.wait_for(handed("b")).await;
    settle().await;
    assert_eq!(b.accepted_inputs(), vec![kv(&[("k", 1)])]);
    assert!(!b.is_dropped().await);
    tokio::time::advance(GRACE).await;
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert!(b.is_dropped().await);
    assert_eq!(checkpoint.nodes(), ["b"]);
}

#[tokio::test(start_paused = true)]
async fn a_creation_that_ends_while_the_run_is_failing_is_dropped_at_the_end_of_the_grace() {
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
async fn a_creation_still_running_at_the_end_of_the_grace_is_dropped_with_its_creation() {
    let rig = single("a");
    let flag = DropFlag::default();
    rig.stage.script("a", Creation::Never(flag.clone()));
    let host = begin(&rig, state(&[]), &["a"], Setup::new());
    settle().await;
    assert!(!flag.is_set());
    host.cancel.trigger();
    settle().await;
    assert!(!flag.is_set());
    tokio::time::advance(GRACE).await;
    let (failures, checkpoint) = cancelled(host.outcome().await);
    settle().await;
    assert!(flag.is_set());
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a"]);
}
