use std::time::Duration;

use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::*;
use crate::observer::*;

#[tokio::test(start_paused = true)]
async fn given_an_input_refused_as_closed_when_the_terminal_event_is_handled_then_a_new_instance_gets_it()
 {
    let rig = a_and_c_into_b();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    b.close_input();
    a.result(set("k", 1));
    host.observer.wait_for(edge_evaluated("a")).await;
    quiesce().await;
    assert_eq!(rig.stage.create_calls("b"), 1);
    assert_eq!(count(&host.observer.log(), handed("b")), 1);
    tokio::time::advance(Duration::from_secs(3600)).await;
    quiesce().await;
    assert_eq!(rig.stage.create_calls("b"), 1);
    assert!(!host.is_finished());
    b.result(set("done", 1));
    let mut second = rig.stage.instance("b").await;
    assert_ne!(second.id, b.id);
    assert_eq!(second.accepted_inputs(), vec![kv(&[("done", 1), ("k", 1)])]);
    host.observer.wait_for_count(handed("b"), 2).await;
    second.result(Change::Empty);
    let observer = host.observer.clone();
    ended(host.outcome().await);
    let log = observer.log();
    assert_eq!(count(&log, handed("b")), 2);
    let second_created = log
        .iter()
        .enumerate()
        .filter(|(_, o)| created("b")(o))
        .map(|(index, _)| index)
        .nth(1)
        .unwrap();
    let last_handed = log.iter().rposition(handed("b")).unwrap();
    assert!(second_created < last_handed);
}

#[tokio::test(start_paused = true)]
async fn given_a_held_input_when_a_second_input_is_refused_then_nothing_is_added_to_it() {
    let rig = a_and_c_into_b();
    let host = begin(&rig, state(&[]), &["a", "b", "c"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    b.close_input();
    a.result(set("from_a", 1));
    host.observer.wait_for(edge_evaluated("a")).await;
    c.result(set("from_c", 1));
    host.observer.wait_for(edge_evaluated("c")).await;
    b.result(Change::Empty);
    let mut second = rig.stage.instance("b").await;
    assert_eq!(
        second.accepted_inputs(),
        vec![kv(&[("from_a", 1), ("from_c", 1)])]
    );
    second.result(Change::Empty);
    ended(host.outcome().await);
    assert_eq!(rig.stage.create_calls("b"), 2);
}

#[tokio::test(start_paused = true)]
async fn given_an_instance_ending_when_an_input_is_given_then_the_input_is_never_lost() {
    let rig = a_and_c_into_b();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    b.result(Change::Empty);
    a.result(set("k", 1));
    let mut second = rig.stage.instance("b").await;
    let inputs = second.accepted_inputs();
    assert_eq!(inputs.len(), 1);
    assert_eq!(value_of(&inputs[0], "k"), Some(1));
    second.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_held_input_when_the_old_instance_fails_then_the_input_is_never_given() {
    let rig = a_and_c_into_b();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    b.close_input();
    a.result(Change::Empty);
    host.observer.wait_for(edge_evaluated("a")).await;
    b.failed("late failure");
    let (failures, checkpoint) = failed(host.outcome().await);
    assert!(matches!(&failures[0], RunFailure::NodeFailed { node, .. } if node == "b"));
    assert_eq!(rig.stage.create_calls("b"), 1);
    assert_eq!(checkpoint.nodes(), ["b"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_live_instance_when_an_input_is_due_then_it_is_given_at_once() {
    let rig = fan("x", &["b"]);
    let host = begin(&rig, state(&[]), &["x", "b"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    let mut b = rig.stage.instance("b").await;
    x.result(set("k", 1));
    host.observer.wait_for_count(handed("b"), 2).await;
    assert_eq!(b.accepted_inputs().len(), 2);
    assert_eq!(rig.stage.create_calls("b"), 1);
    b.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_busy_instance_when_an_input_is_given_then_the_executor_does_not_wait_for_it() {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .edge(routing_on("a", &["a", "b"], |fields| {
            if fields.values.get("n").copied().unwrap_or(0) < 1000 {
                vec!["a", "b"]
            } else {
                vec!["b"]
            }
        }))
        .edge(edge("b", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut b = rig.stage.instance("b").await;
    for _ in 0..1000 {
        let mut a = rig.stage.instance("a").await;
        a.result(add("n", 1));
    }
    host.observer.wait_for_count(handed("b"), 1001).await;
    assert_eq!(b.accepted_inputs().len(), 1001);
    b.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_several_live_instances_when_one_ends_then_no_other_instance_is_dropped() {
    let rig = fan("x", &["b", "c"]);
    let host = begin(&rig, state(&[]), &["x"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    b.result(Change::Empty);
    host.observer.wait_for(edge_evaluated("b")).await;
    quiesce().await;
    assert!(b.is_dropped().await);
    assert!(!c.is_dropped().await);
    c.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_first_input_refused_as_closed_when_the_instance_ends_then_a_new_one_gets_it_once()
{
    let rig = chain("1", &["a"]);
    rig.stage.script("a", Creation::InputClosedAtBirth);
    let host = begin(&rig, state(&[("k", 1)]), &["a"], Setup::new());
    rig.stage.detach();
    let mut first = rig.stage.instance("a").await;
    host.observer.wait_for(created("a")).await;
    quiesce().await;
    assert_eq!(count(&host.observer.log(), handed("a")), 0);
    first.result(set("k", 2));
    let mut second = rig.stage.instance("a").await;
    assert_ne!(second.id, first.id);
    assert_eq!(second.next_input().await, kv(&[("k", 2)]));
    second.result(Change::Empty);
    let observer = host.observer.clone();
    ended(host.outcome().await);
    let log = observer.log();
    assert_eq!(count(&log, created("a")), 2);
    assert_eq!(count(&log, handed("a")), 1);
}
