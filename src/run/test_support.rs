use std::collections::BTreeMap;

use crate::graph::{Edge, FnEdge, Target};
use crate::graph::{FnNode, Node, NodeFuture};
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
        .state(key("count"), Kind::Int)
        .state(key("log"), Kind::list(Kind::Str))
        .state(key("items"), Kind::list(Kind::Str))
        .state(key("item"), Kind::Str)
        .state(key("out"), Kind::Str)
        .state(key("outs"), Kind::list(Kind::Str))
        .state(key("chat"), Kind::Conversation)
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
