use std::sync::Arc;

use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::graph::{Context, FnNode, Graph, GraphBuilder, NodeFuture};
use crate::observe::NoopObserver;
use crate::run::test_support::{config, edge, end, key, nid, noop_node, schema};
use crate::run::{Outcome, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::Key;

fn graph_with(schema: Schema, inputs: &[&str]) -> Graph {
    let mut builder = GraphBuilder::new(schema)
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), edge(vec![end("done")]));
    for name in inputs {
        builder = builder.input(key(name));
    }
    builder.build().unwrap()
}

fn items(values: &[&str]) -> Value {
    Value::list(values.iter().map(|value| Value::str(*value)).collect())
}

#[test]
fn given_declared_inputs_when_start_state_then_inputs_set_and_others_neutral() {
    let graph = graph_with(schema(), &["items", "count"]);
    let state = graph
        .start_state([
            (key("items"), items(&["a", "b"])),
            (key("count"), Value::int(4)),
        ])
        .unwrap();
    assert_eq!(
        state.list(&key("items")).unwrap(),
        &[Value::str("a"), Value::str("b")]
    );
    assert_eq!(state.int(&key("count")).unwrap(), 4);
    assert_eq!(state.str(&key("item")).unwrap(), "");
    assert!(state.list(&key("outs")).unwrap().is_empty());
    assert_eq!(
        state.conversation(&key("chat")).unwrap(),
        &Conversation::new()
    );
}

#[test]
fn given_no_signature_when_start_state_with_nothing_then_every_key_neutral() {
    let graph = graph_with(schema(), &[]);
    let state = graph.start_state(Vec::new()).unwrap();
    assert_eq!(state.int(&key("count")).unwrap(), 0);
    assert!(state.list(&key("log")).unwrap().is_empty());
}

#[test]
fn given_explicit_default_when_start_state_then_key_starts_with_it() {
    let schema = Schema::builder()
        .state_with_default(key("limit"), Kind::Int, Value::int(3))
        .state(key("ratio"), Kind::Float)
        .state(key("flag"), Kind::Bool)
        .build();
    let graph = graph_with(schema, &[]);
    let state = graph.start_state(Vec::new()).unwrap();
    assert_eq!(state.int(&key("limit")).unwrap(), 3);
    assert_eq!(state.float(&key("ratio")).unwrap(), 0.0);
    assert!(!state.bool(&key("flag")).unwrap());
}

#[test]
fn given_a_declared_input_missing_when_start_state_then_missing_input() {
    let graph = graph_with(schema(), &["items", "count"]);
    assert!(matches!(
        graph.start_state([(key("items"), items(&[]))]),
        Err(GraphError::MissingInput { key }) if key == self::key("count")
    ));
}

#[test]
fn given_a_key_that_is_not_an_input_when_start_state_then_not_an_input() {
    let graph = graph_with(schema(), &["items"]);
    assert!(matches!(
        graph.start_state([(key("items"), items(&[])), (key("count"), Value::int(1))]),
        Err(GraphError::NotAnInput { key }) if key == self::key("count")
    ));
}

#[test]
fn given_an_unknown_key_when_start_state_then_unknown_key() {
    let graph = graph_with(schema(), &["items"]);
    assert!(matches!(
        graph.start_state([(key("items"), items(&[])), (key("ghost"), Value::int(1))]),
        Err(GraphError::UnknownKey { .. })
    ));
}

#[test]
fn given_an_input_of_the_wrong_kind_when_start_state_then_kind_mismatch() {
    let graph = graph_with(schema(), &["count"]);
    assert!(matches!(
        graph.start_state([(key("count"), Value::str("one"))]),
        Err(GraphError::KindMismatch { .. })
    ));
}

#[test]
fn given_an_input_given_twice_when_start_state_then_input_given_twice() {
    let graph = graph_with(schema(), &["count"]);
    assert!(matches!(
        graph.start_state([(key("count"), Value::int(1)), (key("count"), Value::int(2))]),
        Err(GraphError::InputGivenTwice { .. })
    ));
}

#[tokio::test]
async fn given_state_from_start_state_when_run_then_nodes_read_the_inputs() {
    let count: Key = key("count");
    let doubling = FnNode::new(move |s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        let doubled = s.int(&key("count")).map(|value| value * 2);
        Box::pin(async move {
            Ok(vec![Update::Set {
                key: key("count"),
                value: Value::int(doubled?),
            }])
        })
    });
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), doubling)
        .edge(nid("a"), edge(vec![end("done")]))
        .input(count.clone())
        .output(count.clone())
        .build()
        .unwrap();
    let state = graph
        .start_state([(count.clone(), Value::int(21))])
        .unwrap();
    let ctx = Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    );
    let (_sender, mut inbox) = channel();
    let Ok(Outcome::Finished { state, .. }) =
        run(&graph, &config(), state, None, &ctx, &mut inbox).await
    else {
        panic!("expected finished");
    };
    assert_eq!(state.int(&count).unwrap(), 42);
}
