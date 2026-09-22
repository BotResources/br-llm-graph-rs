use std::sync::Arc;

use crate::graph::{Context, FnNode, GraphBuilder, Map, Node, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::inbox::channel;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State, Value};
use crate::update::Update;

fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

fn map_graph(body: Box<dyn Node>) -> crate::graph::Graph {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body,
        output: key("out"),
        results: key("outs"),
    };
    GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), edge(vec![end("done")]))
        .build()
        .unwrap()
}

fn seeded(items: Vec<&str>) -> State {
    let mut state = base_state();
    state
        .apply_batch(&[Update::Set {
            key: key("items"),
            value: Value::list(items.iter().map(|s| Value::str(*s)).collect()),
        }])
        .unwrap();
    state
}

async fn run_map(body: Box<dyn Node>, items: Vec<&str>) -> State {
    let graph = map_graph(body);
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config(), seeded(items), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    state
}

fn outs_of(state: &State) -> Vec<String> {
    state
        .list(&key("outs"))
        .unwrap()
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            _ => String::new(),
        })
        .collect()
}

#[tokio::test]
async fn given_map_over_three_items_when_run_then_three_outputs() {
    let state = run_map(Box::new(item_to_out_node()), vec!["a", "b", "c"]).await;
    assert_eq!(outs_of(&state), vec!["A", "B", "C"]);
}

#[tokio::test]
async fn given_map_over_empty_list_when_run_then_no_outputs() {
    let state = run_map(Box::new(item_to_out_node()), Vec::new()).await;
    assert!(outs_of(&state).is_empty());
}

#[tokio::test]
async fn given_map_body_sets_item_key_when_run_then_appends_correct_and_parent_unchanged() {
    let body = FnNode::new(|s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        let up = s.str(&key("item")).map(str::to_uppercase);
        Box::pin(async move {
            Ok(vec![
                Update::Set {
                    key: key("item"),
                    value: Value::str("touched"),
                },
                Update::Set {
                    key: key("out"),
                    value: Value::str(up?),
                },
            ])
        })
    });
    let state = run_map(Box::new(body), vec!["a", "b"]).await;
    assert_eq!(outs_of(&state), vec!["A", "B"]);
    assert_eq!(state.str(&key("item")).unwrap(), "_");
}

#[tokio::test]
async fn given_map_body_omits_output_key_when_run_then_appends_initial_output() {
    let state = run_map(Box::new(noop_node()), vec!["a", "b"]).await;
    assert_eq!(outs_of(&state), vec!["_", "_"]);
}

#[tokio::test]
async fn given_map_body_errors_when_run_then_node_failed_and_state_intact() {
    let graph = map_graph(Box::new(failing_node()));
    let (_sender, mut inbox) = channel();
    let failure = run(
        &graph,
        &config(),
        seeded(vec!["a"]),
        None,
        &ctx(),
        &mut inbox,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::NodeFailed { .. }
    ));
    assert!(
        failure
            .checkpoint
            .state
            .list(&key("outs"))
            .unwrap()
            .is_empty()
    );
}
