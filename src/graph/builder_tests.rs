use super::*;
use crate::error::GraphError;
use crate::graph::edge::{Always, Target};
use crate::graph::map::Map;
use crate::graph::node::{FnNode, Node, NodeFuture};
use crate::state::{Config, Kind, Schema, State};
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn noop() -> impl Node {
    FnNode::new(
        |_state: &State, _config: &Config, _ctx: &_| -> NodeFuture<'_> {
            Box::pin(async { Ok(Vec::new()) })
        },
    )
}

fn done_edge() -> Always {
    Always(Target::End(EndLabel::new("done").unwrap()))
}

fn schema() -> Schema {
    Schema::builder()
        .state(
            crate::value::Key::new("items").unwrap(),
            Kind::list(Kind::Str),
        )
        .state(crate::value::Key::new("item").unwrap(), Kind::Str)
        .state(crate::value::Key::new("out").unwrap(), Kind::Str)
        .state(
            crate::value::Key::new("outs").unwrap(),
            Kind::list(Kind::Str),
        )
        .build()
}

#[test]
fn given_valid_single_node_when_build_then_ok() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .edge(nid("a"), done_edge())
        .build();
    assert!(graph.is_ok());
}

#[test]
fn given_duplicate_id_when_build_then_refused() {
    let result = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .node(nid("a"), noop())
        .edge(nid("a"), done_edge())
        .build();
    assert!(matches!(result, Err(GraphError::DuplicateNode { .. })));
}

#[test]
fn given_unknown_entry_when_build_then_refused() {
    let result = GraphBuilder::new(schema())
        .entry(nid("missing"))
        .node(nid("a"), noop())
        .edge(nid("a"), done_edge())
        .build();
    assert!(matches!(result, Err(GraphError::UnknownEntry { .. })));
}

#[test]
fn given_missing_entry_when_build_then_refused() {
    let result = GraphBuilder::new(schema())
        .node(nid("a"), noop())
        .edge(nid("a"), done_edge())
        .build();
    assert!(matches!(result, Err(GraphError::MissingEntry)));
}

#[test]
fn given_node_without_edge_when_build_then_refused() {
    let result = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .build();
    assert!(matches!(result, Err(GraphError::NodeWithoutEdge { .. })));
}

#[test]
fn given_edge_from_unknown_node_when_build_then_refused() {
    let result = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .edge(nid("a"), done_edge())
        .edge(nid("ghost"), done_edge())
        .build();
    assert!(matches!(
        result,
        Err(GraphError::EdgeFromUnknownNode { .. })
    ));
}

#[test]
fn given_map_with_bad_keys_when_build_then_refused() {
    let map = Map {
        list: crate::value::Key::new("out").unwrap(),
        item: crate::value::Key::new("item").unwrap(),
        body: Box::new(noop()),
        output: crate::value::Key::new("out").unwrap(),
        results: crate::value::Key::new("outs").unwrap(),
    };
    let result = GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), done_edge())
        .build();
    assert!(matches!(result, Err(GraphError::MapKeyMismatch { .. })));
}

#[test]
fn given_map_with_good_keys_when_build_then_ok() {
    let map = Map {
        list: crate::value::Key::new("items").unwrap(),
        item: crate::value::Key::new("item").unwrap(),
        body: Box::new(noop()),
        output: crate::value::Key::new("out").unwrap(),
        results: crate::value::Key::new("outs").unwrap(),
    };
    let result = GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), done_edge())
        .build();
    assert!(result.is_ok());
}
