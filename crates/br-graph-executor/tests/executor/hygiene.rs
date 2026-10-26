use std::sync::Arc;

use tokio::sync::Notify;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::sink::SinkBehavior;

fn two_independent_nodes_to_end() -> Rig {
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
async fn given_a_run_that_ended_when_it_returns_then_no_task_of_the_run_is_left() {
    let baseline = alive_tasks();
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    ended(host.outcome().await);
    assert_eq!(tasks_back_to(baseline).await, baseline);
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_with_a_live_sibling_when_the_run_returns_then_no_task_is_left() {
    let baseline = alive_tasks();
    let rig = two_independent_nodes_to_end();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.failed("boom");
    failed(host.outcome().await);
    assert!(b.is_dropped().await);
    assert_eq!(tasks_back_to(baseline).await, baseline);
}

#[tokio::test(start_paused = true)]
async fn given_a_run_blocked_in_the_sink_when_it_is_dropped_then_the_sink_call_and_every_task_are_gone()
 {
    let baseline = alive_tasks();
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    sink.script(SinkBehavior::Block(Arc::new(Notify::new())));
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    sink.wait_for_calls(1).await;
    assert_eq!(sink.in_flight(), 1);
    host.drop_the_run();
    assert_eq!(tasks_back_to(baseline).await, baseline);
    assert_eq!(sink.in_flight(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_live_instances_when_the_run_is_dropped_then_they_are_dropped_and_no_task_is_left() {
    let baseline = alive_tasks();
    let rig = two_independent_nodes_to_end();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    assert!(!a.is_dropped().await);
    host.drop_the_run();
    a.wait_dropped().await;
    b.wait_dropped().await;
    assert_eq!(tasks_back_to(baseline).await, baseline);
}

#[tokio::test(start_paused = true)]
async fn given_a_long_loop_when_it_runs_then_the_number_of_tasks_stays_bounded() {
    const ROUNDS: i64 = 1_000;
    let baseline = alive_tasks();
    let rig = rig("1")
        .entry("a")
        .node("a")
        .edge(routing_on("a", &["a", "END"], |fields| {
            if value_of(fields, "n").unwrap_or(0) < ROUNDS {
                vec!["a"]
            } else {
                vec!["END"]
            }
        }))
        .build();
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let mut peak = 0;
    for _ in 0..ROUNDS {
        let mut a = rig.stage.instance("a").await;
        a.result(add("n", 1));
        peak = peak.max(alive_tasks());
    }
    assert_eq!(value_of(&ended(host.outcome().await), "n"), Some(ROUNDS));
    assert!(peak < 30, "peak of {peak} tasks alive during the loop");
    assert_eq!(tasks_back_to(baseline).await, baseline);
}

#[tokio::test(start_paused = true)]
async fn given_a_busy_executor_when_a_node_sent_its_event_then_it_does_not_see_dropped_until_it_is_handled()
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
    let host = begin(&rig, state(&[]), &["a", "c"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let gate = Arc::new(Notify::new());
    sink.script(SinkBehavior::Block(gate.clone()));
    let mut a = rig.stage.instance("a").await;
    let mut c = rig.stage.instance("c").await;
    a.result(set("k", 1));
    sink.wait_for_calls(1).await;
    c.result(Change::Empty);
    assert!(!c.is_dropped().await);
    gate.notify_one();
    host.observer.wait_for(crate::observer::handled("c")).await;
    assert!(c.is_dropped().await);
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    ended(host.outcome().await);
}
