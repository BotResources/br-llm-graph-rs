use std::sync::{Arc, Mutex};

use crate::error::{GraphError, NodeFault};
use crate::graph::{Context, FnNode, GraphBuilder, Map, Node, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::inbox::channel;
use crate::run::outcome::{Outcome, RunFailure};
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
        max_concurrency: None,
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

async fn try_map(body: Box<dyn Node>, items: Vec<&str>) -> Result<Outcome, RunFailure> {
    let graph = map_graph(body);
    let (_sender, mut inbox) = channel();
    run(&graph, &config(), seeded(items), None, &ctx(), &mut inbox).await
}

async fn run_map(body: Box<dyn Node>, items: Vec<&str>) -> State {
    let outcome = try_map(body, items).await.map_err(|f| f.error).unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    state
}

fn texts(state: &State, list: &str) -> Vec<String> {
    state
        .list(&key(list))
        .unwrap()
        .iter()
        .map(|v| match v {
            Value::Str(s) => s.clone(),
            _ => String::new(),
        })
        .collect()
}

fn upper_to_outs() -> impl Node {
    FnNode::new(|s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        let up = s.str(&key("item")).map(str::to_uppercase);
        Box::pin(async move {
            Ok(vec![Update::Append {
                key: key("outs"),
                value: Value::str(up?),
            }])
        })
    })
}

fn returned_error(failure: RunFailure) -> GraphError {
    let GraphError::NodeFailed {
        node,
        source: NodeFault::Returned(returned),
    } = failure.error
    else {
        panic!("expected the map node to return an error");
    };
    assert_eq!(node, nid("m"));
    *returned.downcast::<GraphError>().unwrap()
}

#[tokio::test]
async fn given_map_over_three_items_when_run_then_three_appends_in_item_order() {
    let state = run_map(Box::new(upper_to_outs()), vec!["a", "b", "c"]).await;
    assert_eq!(texts(&state, "outs"), vec!["A", "B", "C"]);
}

#[tokio::test]
async fn given_map_over_empty_list_when_run_then_no_body_runs_and_nothing_appended() {
    let calls = Arc::new(Mutex::new(0));
    let counter = calls.clone();
    let body = FnNode::new(move |_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        *counter.lock().unwrap() += 1;
        Box::pin(async { Ok(Vec::new()) })
    });
    let state = run_map(Box::new(body), Vec::new()).await;
    assert!(texts(&state, "outs").is_empty());
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn given_map_body_returning_nothing_when_run_then_nothing_appended() {
    let state = run_map(Box::new(noop_node()), vec!["a", "b"]).await;
    assert!(texts(&state, "outs").is_empty());
    assert_eq!(state.str(&key("item")).unwrap(), "_");
}

#[tokio::test]
async fn given_map_body_returning_a_set_when_run_then_map_fails_and_nothing_is_dropped_silently() {
    let failure = try_map(Box::new(item_to_out_node()), vec!["a"])
        .await
        .err()
        .unwrap();
    assert!(
        failure
            .checkpoint
            .state
            .list(&key("outs"))
            .unwrap()
            .is_empty()
    );
    assert!(matches!(
        returned_error(failure),
        GraphError::MapBodyNotAppend { key } if key == self::key("out")
    ));
}

#[tokio::test]
async fn given_map_body_appending_to_a_key_that_is_not_a_list_when_run_then_map_fails() {
    let body = FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        Box::pin(async {
            Ok(vec![Update::Append {
                key: key("out"),
                value: Value::str("x"),
            }])
        })
    });
    let failure = try_map(Box::new(body), vec!["a"]).await.err().unwrap();
    assert!(matches!(
        returned_error(failure),
        GraphError::AppendNotList { .. }
    ));
}

#[tokio::test]
async fn given_map_body_errors_when_run_then_node_failed_and_state_intact() {
    let failure = try_map(Box::new(failing_node()), vec!["a"])
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

#[tokio::test]
async fn given_map_items_when_run_then_each_body_runs_in_its_own_item_occurrence() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = seen.clone();
    let body = FnNode::new(
        move |_s: &State, _c: &Config, x: &Context| -> NodeFuture<'_> {
            sink.lock().unwrap().push(x.occurrence().to_string());
            Box::pin(async { Ok(Vec::new()) })
        },
    );
    run_map(Box::new(body), vec!["a", "b", "c"]).await;
    let mut seen = seen.lock().unwrap().clone();
    seen.sort();
    assert_eq!(seen, vec!["m[0]", "m[1]", "m[2]"]);
}

#[tokio::test]
async fn given_a_map_run_outside_a_graph_when_run_then_map_without_occurrence() {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(noop_node()),
        max_concurrency: None,
    };
    let state = seeded(vec!["a"]);
    let error = map.run(&state, &config(), &ctx()).await.err().unwrap();
    assert!(matches!(
        error.downcast_ref::<GraphError>(),
        Some(GraphError::MapWithoutOccurrence)
    ));
    let state = seeded(vec!["a"]);
    let updates = map
        .run(&state, &config(), &ctx().for_node(&nid("m")))
        .await
        .unwrap();
    assert!(updates.is_empty());
}
