//! What the description tests share: ids, a panicking node and edge, and the
//! JSON of a description.

use serde_json::Value as Json;

use crate::describe::GraphDescription;
use crate::graph::{Edge, FnNode, Node, NodeFuture, Target};
use crate::state::{Config, State};
use crate::value::{EndLabel, Key, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

pub(crate) fn to(name: &str) -> Target {
    Target::Node(nid(name))
}

pub(crate) fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

pub(crate) fn noop() -> impl Node {
    FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        Box::pin(async { Ok(Vec::new()) })
    })
}

/// A node that panics when it is run, before it gives its future.
pub(crate) fn panicking() -> impl Node {
    FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
        panic!("a description runs no node")
    })
}

/// An edge that panics when it is evaluated, declaring `targets`.
pub(crate) struct PanickingEdge(pub(crate) Vec<Target>);

impl Edge for PanickingEdge {
    fn next(
        &self,
        _state: &State,
        _config: &Config,
    ) -> Result<Vec<Target>, crate::error::GraphError> {
        panic!("a description evaluates no edge")
    }

    fn declared(&self) -> Option<&[Target]> {
        Some(&self.0)
    }
}

/// The JSON of `description`.
pub(crate) fn json(description: &GraphDescription) -> Json {
    serde_json::to_value(description).unwrap()
}

/// The JSON of the node `id` of `description`.
pub(crate) fn node_json(description: &GraphDescription, id: &str) -> Json {
    let json = json(description);
    json["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == id)
        .unwrap_or_else(|| panic!("no node {id}"))
        .clone()
}

/// Asserts that `actual` is written as `expected`, field order included: a
/// description writes its fields in a fixed order, which `Json` equality
/// does not see (its maps compare as sets).
pub(crate) fn assert_written(actual: &Json, expected: &Json) {
    assert_eq!(actual, expected);
    assert_eq!(
        actual.to_string(),
        expected.to_string(),
        "the fields are written in another order"
    );
}
