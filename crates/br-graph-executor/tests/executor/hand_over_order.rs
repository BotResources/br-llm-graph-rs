use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;

#[tokio::test(start_paused = true)]
async fn given_an_edge_declaring_targets_in_an_order_when_handing_over_then_that_order_is_kept() {
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
async fn given_targets_and_a_held_node_when_handing_over_then_the_held_node_follows_the_targets() {
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
async fn given_a_checkpoint_naming_three_nodes_out_of_order_when_resumed_then_they_are_created_in_the_order_of_their_names()
 {
    let rig = rig("1")
        .entry("a")
        .entry("b")
        .entry("c")
        .node("a")
        .node("b")
        .node("c")
        .edge(edge("a", &["END"]))
        .edge(edge("b", &["END"]))
        .edge(edge("c", &["END"]))
        .build();
    let checkpoint = handwritten_checkpoint(&rig, r#"{"values":{},"pad":""}"#, &["c", "a", "b"]);
    let resumed = begin_resume(&rig, checkpoint, BIG, Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    let mut c = rig.stage.instance("c").await;
    assert_eq!(rig.stage.call_order(), ["a", "b", "c"]);
    a.result(Change::Empty);
    b.result(Change::Empty);
    c.result(Change::Empty);
    ended(resumed.outcome().await);
}

#[tokio::test(start_paused = true)]
async fn given_a_target_that_also_holds_an_input_when_handing_over_then_it_is_ordered_with_the_targets()
 {
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
