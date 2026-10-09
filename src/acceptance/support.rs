use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use crate::{
    Always, Config, Context, EndLabel, FnEdge, FnNode, Graph, GraphBuilder, Input, Key, Kind,
    Limit, Map, NodeFuture, NodeId, Observer, OnFailure, Output, Schema, State, SubGraph, Target,
    Update, Value,
};

pub(crate) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

/// Counts how many times each item was prepared, and makes the review of
/// chosen items fail.
#[derive(Default)]
pub(crate) struct Probe {
    prepared: Mutex<BTreeMap<String, usize>>,
    failing: Mutex<BTreeSet<String>>,
}

impl Probe {
    pub(crate) fn failing(items: &[&str]) -> Arc<Self> {
        let probe = Probe::default();
        for item in items {
            probe.failing.lock().unwrap().insert(item.to_uppercase());
        }
        Arc::new(probe)
    }

    pub(crate) fn heal(&self) {
        self.failing.lock().unwrap().clear();
    }

    pub(crate) fn prepared(&self) -> BTreeMap<String, usize> {
        self.prepared.lock().unwrap().clone()
    }
}

/// The number at the end of an item name.
pub(crate) fn number(text: &str) -> usize {
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    digits.parse().unwrap_or(0)
}

/// Rounds the review asks for before it accepts item `n`.
pub(crate) fn needed(n: usize) -> i64 {
    1 + (n % 3) as i64
}

/// A generator/critic loop. `generate` writes a new version of `answer` and
/// counts it in its private `count`; `review` sets its private `flag` once
/// enough versions were written. Lower item numbers take longer.
fn refine_loop(probe: &Arc<Probe>) -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("text"), Kind::Str)
        .state(key("count"), Kind::Int)
        .state(key("flag"), Kind::Bool)
        .state(key("answer"), Kind::Str)
        .build();
    let generate = FnNode::new(|s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        Box::pin(async move {
            let text = s.str(&key("text"))?;
            for _ in 0..(20_usize.saturating_sub(number(text))) {
                tokio::task::yield_now().await;
            }
            let count = s.int(&key("count"))? + 1;
            Ok(vec![
                Update::Set {
                    key: key("count"),
                    value: Value::int(count),
                },
                Update::Set {
                    key: key("answer"),
                    value: Value::str(format!("{text} v{count}")),
                },
            ])
        })
    });
    let critic = probe.clone();
    let review = FnNode::new(
        move |s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let critic = critic.clone();
            Box::pin(async move {
                let text = s.str(&key("text"))?;
                if critic.failing.lock().unwrap().contains(text) {
                    return Err(format!("critic unavailable for {text}").into());
                }
                let count = s.int(&key("count"))?;
                Ok(vec![Update::Set {
                    key: key("flag"),
                    value: Value::bool(count >= needed(number(text))),
                }])
            })
        },
    );
    let route = FnEdge::new(|s: &State, _c: &Config| {
        if s.bool(&key("flag"))? {
            Ok(vec![end("accepted")])
        } else {
            Ok(vec![Target::Node(nid("generate"))])
        }
    });
    let graph = GraphBuilder::new(schema)
        .entry(nid("generate"))
        .node(nid("generate"), generate)
        .node(nid("review"), review)
        .edge(nid("generate"), Always(Target::Node(nid("review"))))
        .edge(nid("review"), route)
        .input(key("text"))
        .output(key("answer"))
        .output(key("count"))
        .build()
        .unwrap();
    Arc::new(graph)
}

/// `prepare` turns the item into the text to refine, then `refine` calls the
/// loop. Input `item`; outputs `analysis` and `attempts`.
fn wrapper(probe: &Arc<Probe>) -> Arc<Graph> {
    let schema = Schema::builder()
        .state(key("item"), Kind::Str)
        .state(key("text"), Kind::Str)
        .state(key("analysis"), Kind::Str)
        .state(key("attempts"), Kind::Int)
        .build();
    let counter = probe.clone();
    let prepare = FnNode::new(
        move |s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let counter = counter.clone();
            Box::pin(async move {
                let item = s.str(&key("item"))?;
                *counter
                    .prepared
                    .lock()
                    .unwrap()
                    .entry(item.to_owned())
                    .or_default() += 1;
                Ok(vec![Update::Set {
                    key: key("text"),
                    value: Value::str(item.to_uppercase()),
                }])
            })
        },
    );
    let refine = SubGraph::call(refine_loop(probe))
        .input(key("text"), Input::From(key("text")))
        .output(key("answer"), Output::Set(key("analysis")))
        .output(key("count"), Output::Set(key("attempts")));
    let graph = GraphBuilder::new(schema)
        .entry(nid("prepare"))
        .node(nid("prepare"), prepare)
        .subgraph(nid("refine"), refine)
        .edge(nid("prepare"), Always(Target::Node(nid("refine"))))
        .edge(nid("refine"), Always(end("done")))
        .input(key("item"))
        .output(key("analysis"))
        .output(key("attempts"))
        .build()
        .unwrap();
    Arc::new(graph)
}

/// The caller's schema: its own `count` and `flag` share their names with the
/// private keys of the loop.
pub(crate) fn caller_schema() -> Schema {
    Schema::builder()
        .state(key("items"), Kind::list(Kind::Str))
        .state(key("item"), Kind::Str)
        .state(key("analyses"), Kind::list(Kind::Str))
        .state(key("attempts"), Kind::list(Kind::Int))
        .state(key("failed_items"), Kind::list(Kind::Str))
        .state(key("failures"), Kind::list(Kind::Str))
        .state(key("count"), Kind::Int)
        .state(key("flag"), Kind::Bool)
        .build()
}

pub(crate) fn per_item(probe: &Arc<Probe>) -> SubGraph {
    SubGraph::call(wrapper(probe))
        .input(key("item"), Input::From(key("item")))
        .output(key("analysis"), Output::Append(key("analyses")))
        .output(key("attempts"), Output::Append(key("attempts")))
}

/// A graph whose single node `each` maps `body` over `items`, five at a time.
pub(crate) fn caller(body: SubGraph) -> Result<Graph, crate::GraphError> {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(5).unwrap())),
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    GraphBuilder::new(caller_schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(nid("each"), Always(end("done")))
        .input(key("items"))
        .input(key("count"))
        .input(key("flag"))
        .build()
}

pub(crate) fn capture_failures(body: SubGraph) -> SubGraph {
    body.on_failure(OnFailure::Capture(vec![
        crate::CaptureUpdate::Append(key("failed_items"), crate::CaptureSource::From(key("item"))),
        crate::CaptureUpdate::Append(key("failures"), crate::CaptureSource::Reason),
    ]))
}

pub(crate) fn item_names(count: usize) -> Vec<String> {
    (0..count).map(|n| format!("item{n:02}")).collect()
}

pub(crate) fn start(graph: &Graph, count: usize) -> State {
    let items = Value::list(item_names(count).into_iter().map(Value::str).collect());
    graph
        .start_state([
            (key("items"), items),
            (key("count"), Value::int(7)),
            (key("flag"), Value::bool(true)),
        ])
        .unwrap()
}

pub(crate) fn config(graph: &Graph) -> Config {
    Config::new(graph.schema(), BTreeMap::new()).unwrap()
}

pub(crate) fn context(observer: Arc<dyn Observer>) -> Context {
    crate::testkit::context(observer)
}

pub(crate) fn texts(state: &State, list: &str) -> Vec<String> {
    state
        .list(&key(list))
        .unwrap()
        .iter()
        .map(|value| match value {
            Value::Str(text) => text.clone(),
            other => other.to_string(),
        })
        .collect()
}

pub(crate) fn expected_analysis(name: &str) -> String {
    format!("{} v{}", name.to_uppercase(), needed(number(name)))
}
