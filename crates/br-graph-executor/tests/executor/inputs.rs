use std::time::Duration;

use br_graph_executor::RunFailure;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;

fn two_sources_into_b() -> Rig {
    rig("1")
        .entry("a")
        .entry("b")
        .entry("c")
        .node("a")
        .node("b")
        .node("c")
        .edge(edge("a", &["b"]))
        .edge(edge("c", &["b"]))
        .edge(edge("b", &["END"]))
        .build()
}

#[tokio::test(start_paused = true)]
async fn an_input_refused_as_closed_waits_for_the_terminal_event_and_goes_to_a_new_instance() {
    let rig = two_sources_into_b();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    b.close_input();
    a.result(set("k", 1));
    host.observer.wait_for(edge_evaluated("a")).await;
    settle().await;
    assert_eq!(rig.stage.create_calls("b"), 1);
    assert_eq!(count(&host.observer.log(), handed("b")), 1);
    tokio::time::advance(Duration::from_secs(3600)).await;
    settle().await;
    assert_eq!(rig.stage.create_calls("b"), 1);
    assert!(!host.is_finished());
    b.result(set("done", 1));
    let mut second = rig.stage.instance("b").await;
    assert_ne!(second.id, b.id);
    assert_eq!(
        second.accepted_inputs(),
        vec![kv(&[("done", 1), ("k", 1)])]
    );
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
async fn a_second_refused_input_adds_nothing_to_the_held_one() {
    let rig = two_sources_into_b();
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
async fn an_input_given_as_the_instance_ends_is_never_lost() {
    let rig = two_sources_into_b();
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
async fn a_held_input_whose_old_instance_fails_is_never_given() {
    let rig = two_sources_into_b();
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
async fn an_input_for_a_live_instance_is_given_at_once_whatever_it_is_doing() {
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
async fn giving_an_input_never_waits_for_the_instance() {
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
async fn the_targets_are_handed_over_in_the_order_the_edge_declared_them() {
    let rig = rig("1")
        .entry("b")
        .entry("c")
        .entry("x")
        .node("b")
        .node("c")
        .node("x")
        .edge(edge("x", &["c", "b"]))
        .edge(edge("b", &["END"]))
        .edge(edge("c", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["b", "c", "x"], Setup::new());
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    host.observer.wait_for_count(handed("b"), 2).await;
    let log = host.observer.log();
    let evaluated = position(&log, edge_evaluated("x"));
    let order: Vec<_> = log[evaluated..]
        .iter()
        .filter_map(|o| match o {
            br_graph_executor::Observation::InputHandedOver { node, .. } => Some(node.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["c", "b"]);
    b.result(Change::Empty);
    c.result(Change::Empty);
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn the_other_held_nodes_follow_the_targets_in_the_order_of_their_names() {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .node("c")
        .node("d")
        .edge(routing_on("a", &["d", "c", "END"], |fields| {
            if fields.values.contains_key("second") {
                vec!["END"]
            } else {
                vec!["d", "c"]
            }
        }))
        .edge(edge("b", &["a"]))
        .edge(edge("c", &["END"]))
        .edge(edge("d", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.close_input();
    b.result(Change::Empty);
    host.observer.wait_for(edge_evaluated("b")).await;
    a.result(Change::Empty);
    let mut c = rig.stage.instance("c").await;
    let mut d = rig.stage.instance("d").await;
    let mut a_again = rig.stage.instance("a").await;
    assert_eq!(rig.stage.call_order()[2..], ["d", "c", "a"]);
    c.result(Change::Empty);
    d.result(Change::Empty);
    a_again.result(set("second", 1));
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn a_node_that_is_a_target_and_held_is_ordered_with_the_targets() {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .node("a")
        .node("b")
        .node("c")
        .edge(routing_on("a", &["c", "a", "END"], |fields| {
            if fields.values.contains_key("second") {
                vec!["END"]
            } else {
                vec!["c", "a"]
            }
        }))
        .edge(edge("b", &["a"]))
        .edge(edge("c", &["END"]))
        .build();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.close_input();
    b.result(Change::Empty);
    host.observer.wait_for(edge_evaluated("b")).await;
    a.result(Change::Empty);
    let mut c = rig.stage.instance("c").await;
    let mut a_again = rig.stage.instance("a").await;
    assert_eq!(rig.stage.call_order()[2..], ["c", "a"]);
    c.result(Change::Empty);
    a_again.result(set("second", 1));
    ended(host.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn the_executor_drops_no_instance_but_the_one_whose_terminal_event_it_handles() {
    let rig = fan("x", &["b", "c"]);
    let host = begin(&rig, state(&[]), &["x"], Setup::new());
    let mut x = rig.stage.instance("x").await;
    x.result(Change::Empty);
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    b.result(Change::Empty);
    host.observer.wait_for(edge_evaluated("b")).await;
    settle().await;
    assert!(b.is_dropped().await);
    assert!(!c.is_dropped().await);
    c.result(Change::Empty);
    ended(host.outcome().await);
}
