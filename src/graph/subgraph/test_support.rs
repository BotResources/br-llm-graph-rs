use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::subgraph::{Input, Output, SubGraph};
use crate::graph::{Context, FnEdge, FnNode, Graph, GraphBuilder, NodeFuture, Target};
use crate::observe::{NoopObserver, Observer};
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

pub(crate) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

/// A small refinement loop: `draft` writes `answer` from `text` and its private
/// counter until the counter reaches the `rounds` configuration, then sets its
/// private flag and ends with `approved`. Inputs: `text`. Outputs: `answer`,
/// `count`. Configuration: `rounds`.
pub(crate) fn refine_graph() -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("text"), Kind::Str)
        .state(key("count"), Kind::Int)
        .state(key("flag"), Kind::Bool)
        .state(key("answer"), Kind::Str)
        .config(key("rounds"), Kind::Int)
        .build();
    let draft = FnNode::new(|s: &State, c: &Config, _x: &Context| -> NodeFuture<'_> {
        Box::pin(async move {
            let count = s.int(&key("count"))? + 1;
            let text = s.str(&key("text"))?;
            Ok(vec![
                Update::Set {
                    key: key("count"),
                    value: Value::int(count),
                },
                Update::Set {
                    key: key("answer"),
                    value: Value::str(format!("{text}#{count}")),
                },
                Update::Set {
                    key: key("flag"),
                    value: Value::bool(count >= c.int(&key("rounds"))?),
                },
            ])
        })
    });
    let route = FnEdge::new(|s: &State, _c: &Config| {
        if s.bool(&key("flag"))? {
            Ok(vec![end("approved")])
        } else {
            Ok(vec![Target::Node(nid("draft"))])
        }
    });
    let graph = GraphBuilder::new(schema)
        .entry(nid("draft"))
        .node(nid("draft"), draft)
        .edge(nid("draft"), route)
        .input(key("text"))
        .output(key("answer"))
        .output(key("count"))
        .build()
        .unwrap();
    Arc::new(graph)
}

/// The caller's schema: `count` and `flag` share their names with the private
/// keys of the refinement loop.
pub(crate) fn parent_schema() -> Schema {
    Schema::builder()
        .state(key("question"), Kind::Str)
        .state(key("count"), Kind::Int)
        .state(key("flag"), Kind::Bool)
        .state(key("result"), Kind::Str)
        .state(key("steps"), Kind::Int)
        .state(key("results"), Kind::list(Kind::Str))
        .state(key("label"), Kind::Str)
        .state(key("labels"), Kind::list(Kind::Str))
        .config(key("rounds"), Kind::Int)
        .config(key("tone"), Kind::Str)
        .build()
}

/// The usual call: `text` from `question`, `rounds` from the caller's
/// configuration, `answer` into `result`.
pub(crate) fn refine_call() -> SubGraph {
    SubGraph::call(refine_graph())
        .input(key("text"), Input::From(key("question")))
        .config(key("rounds"), Input::Config(key("rounds")))
        .output(key("answer"), Output::Set(key("result")))
}

/// A caller whose single node `ask` makes `call` and ends with `done`.
pub(crate) fn parent_with(call: SubGraph) -> Result<Graph, GraphError> {
    GraphBuilder::new(parent_schema())
        .entry(nid("ask"))
        .subgraph(nid("ask"), call)
        .edge(nid("ask"), crate::graph::Always(end("done")))
        .input(key("question"))
        .input(key("count"))
        .input(key("flag"))
        .build()
}

pub(crate) fn parent_state(graph: &Graph, question: &str) -> State {
    graph
        .start_state([
            (key("question"), Value::str(question)),
            (key("count"), Value::int(100)),
            (key("flag"), Value::bool(true)),
        ])
        .unwrap()
}

pub(crate) fn parent_config(rounds: i64) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("rounds"), Value::int(rounds));
    values.insert(key("tone"), Value::str("plain"));
    Config::new(&parent_schema(), values).unwrap()
}

pub(crate) fn context(observer: Arc<dyn Observer>) -> Context {
    crate::testkit::context(observer)
}

pub(crate) async fn run_parent(
    graph: &Graph,
    state: State,
    config: &Config,
    observer: Arc<dyn Observer>,
) -> Result<Outcome, RunFailure> {
    let (_sender, mut inbox) = channel();
    run(graph, config, state, None, &context(observer), &mut inbox).await
}

pub(crate) async fn finished(graph: &Graph, question: &str, rounds: i64) -> State {
    let state = parent_state(graph, question);
    match run_parent(graph, state, &parent_config(rounds), Arc::new(NoopObserver)).await {
        Ok(Outcome::Finished { state, .. }) => state,
        Ok(_) => panic!("the run did not finish"),
        Err(failure) => panic!("the run failed: {}", failure.error),
    }
}
