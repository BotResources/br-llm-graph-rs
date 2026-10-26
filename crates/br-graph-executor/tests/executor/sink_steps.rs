use std::sync::Arc;

use br_graph_executor::RunFailure;
use tokio::sync::Notify;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;
use crate::sink::SinkBehavior;

fn a_then_b_with_c_alone() -> Rig {
    rig("1")
        .entry("a")
        .entry("c")
        .node("a")
        .node("b")
        .node("c")
        .edge(edge("a", &["b"]))
        .edge(edge("b", &["END"]))
        .edge(edge("c", &["END"]))
        .build()
}

#[tokio::test(start_paused = true)]
async fn given_steps_with_updates_when_the_run_goes_on_then_the_sink_gets_all_but_the_last_checkpoint()
 {
    let rig = chain("1", &["a", "b", "c", "d"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    assert_eq!(sink.calls(), 0);
    for (position, name) in ["a", "b", "c"].iter().enumerate() {
        let mut instance = rig.stage.instance(name).await;
        assert_eq!(sink.calls(), position);
        instance.result(add("n", 1));
        sink.wait_for_calls(position + 1).await;
    }
    let mut d = rig.stage.instance("d").await;
    d.result(add("n", 1));
    assert_eq!(value_of(&ended(host.outcome().await), "n"), Some(4));
    assert_eq!(sink.calls(), 3);
}

#[tokio::test(start_paused = true)]
async fn given_a_step_without_an_applied_update_when_handled_then_the_sink_is_not_called() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    host.observer.wait_for_count(handed("a"), 1).await;
    quiesce().await;
    assert_eq!(sink.calls(), 0);
    a.result(Change::Refuse);
    host.observer.wait_for(handled("a")).await;
    host.cancel.trigger();
    quiesce().await;
    b.failed("boom");
    failed(host.outcome().await);
    assert_eq!(sink.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_step_when_its_checkpoint_is_built_then_it_names_targets_and_live_nodes_with_every_update()
 {
    let rig = a_then_b_with_c_alone();
    let host = begin(&rig, state(&[]), &["a", "c"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let mut a = rig.stage.instance("a").await;
    let mut c = rig.stage.instance("c").await;
    a.result(set("k", 1));
    sink.wait_for_calls(1).await;
    let mut b = rig.stage.instance("b").await;
    b.result(set("j", 2));
    sink.wait_for_calls(2).await;
    c.failed("boom");
    let (_, outcome_checkpoint) = failed(host.outcome().await);
    let received = sink.received();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].nodes(), ["b", "c"]);
    assert_eq!(state_of(&received[0]), kv(&[("k", 1)]));
    assert_eq!(received[1].nodes(), ["c"]);
    assert_eq!(state_of(&received[1]), kv(&[("j", 2), ("k", 1)]));
    assert_eq!(outcome_checkpoint.nodes(), ["c"]);
    assert_eq!(state_of(&outcome_checkpoint), kv(&[("j", 2), ("k", 1)]));
}

#[tokio::test(start_paused = true)]
async fn given_a_sink_that_works_when_events_arrive_then_none_is_handled_until_it_answers() {
    let rig = a_then_b_with_c_alone();
    let host = begin(&rig, state(&[]), &["a", "c"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let gate = Arc::new(Notify::new());
    sink.script(SinkBehavior::Block(gate.clone()));
    let mut a = rig.stage.instance("a").await;
    let mut c = rig.stage.instance("c").await;
    a.result(set("k", 1));
    sink.wait_for_calls(1).await;
    c.result(set("j", 1));
    quiesce().await;
    assert_eq!(rig.stage.create_calls("b"), 0);
    assert!(!host.observer.log().iter().any(handled("c")));
    assert!(!c.is_dropped().await);
    assert_eq!(sink.received()[0].nodes(), ["b", "c"]);
    gate.notify_one();
    host.observer.wait_for(handled("c")).await;
    let log = host.observer.log();
    assert!(position(&log, update_applied("a")) < position(&log, handled("c")));
    assert!(position(&log, edge_evaluated("a")) < position(&log, handled("c")));
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    ended(host.outcome().await);
    assert_eq!(sink.overlaps(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_in_the_sink_call_when_the_sink_answers_then_nothing_is_handed_over() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let gate = Arc::new(Notify::new());
    sink.script(SinkBehavior::Block(gate.clone()));
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    sink.wait_for_calls(1).await;
    host.cancel.trigger();
    quiesce().await;
    gate.notify_one();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(rig.stage.create_calls("b"), 0);
    assert_eq!(checkpoint.nodes(), ["b"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_run_ended_when_the_last_stored_checkpoint_is_resumed_then_the_last_node_runs_again()
 {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let mut b = rig.stage.instance("b").await;
    b.result(set("done", 1));
    ended(host.outcome().await);
    let last = sink.received().last().cloned().unwrap();
    assert_eq!(last.nodes(), ["b"]);
    let resumed = begin_resume(&rig, last, BIG, Setup::new());
    let mut again = rig.stage.instance("b").await;
    assert_eq!(again.accepted_inputs(), vec![kv(&[("k", 1)])]);
    again.result(Change::Empty);
    ended(resumed.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusing_sink_when_called_then_the_run_fails_and_the_outcome_holds_the_refused_checkpoint()
 {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    sink.script(SinkBehavior::Refuse("disk full"));
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let (failures, checkpoint) = failed(host.outcome().await);
    match &failures[0] {
        RunFailure::SinkRefused { reason, .. } => assert!(reason.contains("disk full")),
        other => panic!("unexpected failure {other:?}"),
    }
    assert!(failures[0].to_string().contains("disk full"));
    assert_eq!(checkpoint, sink.received()[0]);
    assert_eq!(rig.stage.create_calls("b"), 0);
    assert_eq!(checkpoint.nodes(), ["b"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_sink_that_panics_when_called_then_it_counts_as_a_refusal() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    host.sink.clone().unwrap().script(SinkBehavior::Panic);
    let mut a = rig.stage.instance("a").await;
    a.result(Change::Empty);
    let (failures, _) = failed(host.outcome().await);
    match &failures[0] {
        RunFailure::SinkRefused { reason, .. } => assert!(reason.contains("the sink panics")),
        other => panic!("unexpected failure {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_refusal_when_a_result_is_applied_in_grace_then_the_outcome_checkpoint_is_the_newest()
 {
    let rig = rig("1")
        .entry("p")
        .entry("f")
        .node("p")
        .node("q")
        .node("f")
        .edge(edge("p", &["q"]))
        .edge(edge("q", &["END"]))
        .edge(edge("f", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["p", "f"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    sink.script(SinkBehavior::Refuse("disk full"));
    let mut p = rig.stage.instance("p").await;
    let mut f = rig.stage.instance("f").await;
    p.result(set("k", 1));
    host.observer.wait_for(handled("p")).await;
    sink.wait_for_calls(1).await;
    quiesce().await;
    f.result(set("late", 9));
    let (failures, checkpoint) = failed(host.outcome().await);
    match &failures[0] {
        RunFailure::SinkRefused {
            checkpoint: refused,
            ..
        } => {
            assert_eq!(refused, &sink.received()[0]);
            assert_ne!(refused, &checkpoint);
            assert_eq!(state_of(refused), kv(&[("k", 1)]));
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(state_of(&sink.received()[0]), kv(&[("k", 1)]));
    assert_eq!(state_of(&checkpoint), kv(&[("k", 1), ("late", 9)]));
    assert_ne!(checkpoint, sink.received()[0]);
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_when_a_result_is_applied_in_the_grace_then_the_sink_is_not_called_again() {
    let rig = rig("1")
        .entry("p")
        .entry("f")
        .node("p")
        .node("q")
        .node("f")
        .edge(edge("p", &["q"]))
        .edge(edge("q", &["END"]))
        .edge(edge("f", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["p", "f"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let mut p = rig.stage.instance("p").await;
    let mut f = rig.stage.instance("f").await;
    p.result(set("k", 1));
    sink.wait_for_calls(1).await;
    let mut q = rig.stage.instance("q").await;
    f.failed("boom");
    host.observer.wait_for(handled("f")).await;
    q.result(set("k", 2));
    let (_, checkpoint) = failed(host.outcome().await);
    assert_eq!(sink.calls(), 1);
    assert_eq!(state_of(&sink.received()[0]), kv(&[("k", 1)]));
    assert_eq!(state_of(&checkpoint), kv(&[("k", 2)]));
}
