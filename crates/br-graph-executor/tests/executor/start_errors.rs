use br_graph_executor::{StartError, start};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;

async fn refusal(nodes: &[&str]) -> StartError {
    let rig = chain("1", &["a", "b"]);
    let (config, ..) = Setup::new().config();
    let nodes = nodes.iter().map(|name| (*name).to_owned()).collect();
    within(start(rig.graph, state(&[]), nodes, config))
        .await
        .unwrap_err()
}

#[tokio::test(start_paused = true)]
async fn an_empty_list_of_nodes_is_refused() {
    assert!(matches!(refusal(&[]).await, StartError::NoNodes));
}

#[tokio::test(start_paused = true)]
async fn a_name_that_is_not_a_node_of_the_graph_is_refused() {
    assert!(matches!(
        refusal(&["a", "zz"]).await,
        StartError::UnknownNode { name } if name == "zz"
    ));
}

#[tokio::test(start_paused = true)]
async fn a_name_listed_twice_is_refused() {
    assert!(matches!(
        refusal(&["a", "a"]).await,
        StartError::DuplicateNode { name } if name == "a"
    ));
}

#[tokio::test(start_paused = true)]
async fn every_refusal_carries_a_reason_that_names_the_node() {
    assert!(!refusal(&[]).await.to_string().is_empty());
    assert!(refusal(&["zz"]).await.to_string().contains("zz"));
    assert!(refusal(&["b", "b"]).await.to_string().contains('b'));
}

#[tokio::test(start_paused = true)]
async fn a_refused_start_creates_no_instance() {
    let rig = chain("1", &["a", "b"]);
    let (config, ..) = Setup::new().config();
    let refused = within(start(
        rig.graph.clone(),
        state(&[]),
        vec!["a".into(), "zz".into()],
        config,
    ))
    .await;
    assert!(refused.is_err());
    quiesce().await;
    assert_eq!(rig.stage.create_calls("a"), 0);
}

#[tokio::test(start_paused = true)]
async fn a_node_of_the_middle_of_the_graph_can_be_started() {
    let rig = chain("1", &["a", "b", "c"]);
    let host = begin(&rig, state(&[("k", 1)]), &["b"], Setup::new());
    let mut b = rig.stage.instance("b").await;
    assert_eq!(b.next_input().await, kv(&[("k", 1)]));
    b.result(set("k", 2));
    let mut c = rig.stage.instance("c").await;
    c.result(add("k", 1));
    assert_eq!(ended(host.outcome().await), kv(&[("k", 3)]));
    assert_eq!(rig.stage.create_calls("a"), 0);
}

#[tokio::test(start_paused = true)]
async fn all_nodes_can_be_started_together_and_each_gets_one_input() {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[("k", 7)]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    assert_eq!(a.accepted_inputs(), vec![kv(&[("k", 7)])]);
    assert_eq!(b.accepted_inputs(), vec![kv(&[("k", 7)])]);
    a.result(add("a", 1));
    host.observer.wait_for_count(handed("b"), 2).await;
    assert_eq!(b.accepted_inputs().len(), 1);
    b.result(add("b", 1));
    let final_state = ended(host.outcome().await);
    assert_eq!(value_of(&final_state, "a"), Some(1));
    assert_eq!(value_of(&final_state, "b"), Some(1));
    assert_eq!(rig.stage.create_calls("b"), 1);
}
