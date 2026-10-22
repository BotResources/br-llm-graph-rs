use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use br_llm_messages::{InputId, Text, UserBlock, UserInput, UserSource};

use crate::graph::{Context, Edge, FnEdge, Target};
use crate::graph::{FnNode, Node, NodeFuture};
use crate::observe::{NodeOutcome, NoopObserver, Observer};
use crate::origin::Origin;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

pub(crate) fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

pub(crate) fn to(name: &str) -> Target {
    Target::Node(nid(name))
}

pub(crate) fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("log"), Kind::list(Kind::Str))
        .state_by_name(key("items"), Kind::list(Kind::Str))
        .state_by_name(key("item"), Kind::Str)
        .state_by_name(key("out"), Kind::Str)
        .state_by_name(key("outs"), Kind::list(Kind::Str))
        .state_by_name(key("chat"), Kind::Conversation)
        .build()
}

pub(crate) fn base_state() -> State {
    let mut values = BTreeMap::new();
    values.insert(key("count"), Value::int(0));
    values.insert(key("log"), Value::list(Vec::new()));
    values.insert(key("items"), Value::list(Vec::new()));
    values.insert(key("item"), Value::str("_"));
    values.insert(key("out"), Value::str("_"));
    values.insert(key("outs"), Value::list(Vec::new()));
    values.insert(
        key("chat"),
        Value::conversation(br_llm_messages::Conversation::new()),
    );
    State::new(schema(), values).unwrap()
}

pub(crate) fn config() -> Config {
    Config::new(&schema(), BTreeMap::new()).unwrap()
}

/// A state and a configuration made from schemas that are not `schema()`.
pub(crate) fn foreign_state_and_config() -> (State, Config) {
    let foreign = Schema::builder()
        .state_by_name(key("count"), Kind::Int)
        .config_by_name(key("width"), Kind::Int)
        .build();
    let state = State::new(
        foreign.clone(),
        BTreeMap::from([(key("count"), Value::int(0))]),
    )
    .unwrap();
    let config = Config::new(&foreign, BTreeMap::from([(key("width"), Value::int(2))])).unwrap();
    (state, config)
}

pub(crate) fn noop_node() -> impl Node {
    FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        Box::pin(async { Ok(Vec::new()) })
    })
}

pub(crate) fn log_node(mark: &str) -> impl Node {
    let mark = mark.to_owned();
    FnNode::new(move |_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        let value = Value::str(mark.clone());
        Box::pin(async move {
            Ok(vec![Update::Append {
                key: key("log"),
                value,
            }])
        })
    })
}

pub(crate) fn set_count_node(value: i64) -> impl Node {
    FnNode::new(move |_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        Box::pin(async move {
            Ok(vec![Update::Set {
                key: key("count"),
                value: Value::int(value),
            }])
        })
    })
}

async fn boom() -> Result<Vec<Update>, crate::graph::NodeError> {
    Err("boom".into())
}

async fn kaboom() -> Result<Vec<Update>, crate::graph::NodeError> {
    panic!("kaboom")
}

pub(crate) fn failing_node() -> impl Node {
    FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> { Box::pin(boom()) })
}

pub(crate) fn panicking_node() -> impl Node {
    FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> { Box::pin(kaboom()) })
}

pub(crate) fn item_to_out_node() -> impl Node {
    FnNode::new(|s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        Box::pin(async move {
            let value = s.str(&key("item"))?.to_uppercase();
            Ok(vec![Update::Set {
                key: key("out"),
                value: Value::str(value),
            }])
        })
    })
}

pub(crate) fn edge(targets: Vec<Target>) -> impl Edge {
    FnEdge::new(move |_s: &State, _c: &Config| Ok(targets.clone()))
}

/// A context with no observer and sequential ids.
pub(crate) fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

/// A human input of `body`, its id `i-{body}`.
pub(crate) fn input(body: &str) -> UserInput {
    UserInput::new(
        InputId::new(format!("i-{body}")).unwrap(),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

/// An observer of the node finished event: each node, as `occurrence|node`,
/// with how it ended.
#[derive(Default)]
pub(crate) struct Finishes(Mutex<Vec<(String, NodeOutcome)>>);

impl Finishes {
    pub(crate) fn seen(&self) -> Vec<(String, NodeOutcome)> {
        self.0.lock().unwrap().clone()
    }
}

impl Observer for Finishes {
    fn node_finished(&self, origin: &Origin, node: &NodeId, outcome: NodeOutcome) {
        let line = format!("{}|{node}", origin.occurrence);
        self.0.lock().unwrap().push((line, outcome));
    }
}

/// The node finished events `events` names, as `Finishes` sees them.
pub(crate) fn finished(events: &[(&str, NodeOutcome)]) -> Vec<(String, NodeOutcome)> {
    events
        .iter()
        .map(|(line, outcome)| ((*line).to_owned(), *outcome))
        .collect()
}
