//! The declared node targets of an edge must be nodes of the graph
//! (`EdgeToUnknownNode`), checked at build after the nodes; undeclared
//! targets and end labels are not checked.

use super::*;
use crate::error::GraphError;
use crate::graph::edge::{Always, FnEdge, Target};
use crate::graph::map::Map;
use crate::graph::node::{FnNode, Node, NodeFuture};
use crate::state::{Config, Kind, Schema, State};
use crate::value::{EndLabel, Key, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn to(name: &str) -> Target {
    Target::Node(nid(name))
}

fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).unwrap())
}

fn noop() -> impl Node {
    FnNode::new(
        |_state: &State, _config: &Config, _ctx: &_| -> NodeFuture<'_> {
            Box::pin(async { Ok(Vec::new()) })
        },
    )
}

fn schema() -> Schema {
    Schema::builder()
        .state_by_name(Key::new("item").unwrap(), Kind::Str)
        .build()
}

fn refused(builder: GraphBuilder) -> (NodeId, NodeId) {
    match builder.build() {
        Err(GraphError::EdgeToUnknownNode { node, target }) => (node, target),
        Err(other) => panic!("expected EdgeToUnknownNode, got {other:?}"),
        Ok(_) => panic!("expected EdgeToUnknownNode, the graph was built"),
    }
}

#[test]
fn given_always_to_a_missing_node_when_build_then_edge_to_unknown_node() {
    let builder = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .edge(nid("a"), Always(to("ghost")));
    assert_eq!(refused(builder), (nid("a"), nid("ghost")));
}

#[test]
fn given_a_declared_edge_to_a_missing_node_when_build_then_edge_to_unknown_node() {
    let edge = FnEdge::new(|_: &State, _: &Config| Ok(vec![end("done")])).targets([
        end("done"),
        to("a"),
        to("ghost"),
        to("phantom"),
    ]);
    let builder = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .edge(nid("a"), edge);
    assert_eq!(refused(builder), (nid("a"), nid("ghost")));
}

#[test]
fn given_two_edges_to_missing_nodes_when_build_then_the_first_node_in_registration_order() {
    let builder = GraphBuilder::new(schema())
        .entry(nid("b"))
        .node(nid("b"), noop())
        .node(nid("a"), noop())
        .edge(nid("a"), Always(to("ghost_of_a")))
        .edge(nid("b"), Always(to("ghost_of_b")));
    assert_eq!(refused(builder), (nid("b"), nid("ghost_of_b")));
}

#[test]
fn given_a_target_registered_after_its_edge_when_build_then_accepted() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .edge(nid("a"), Always(to("b")))
        .node(nid("a"), noop())
        .edge(nid("b"), Always(end("done")))
        .node(nid("b"), noop())
        .build();
    assert!(graph.is_ok());
}

#[test]
fn given_an_undeclared_edge_to_a_missing_node_when_build_then_not_checked() {
    let edge = FnEdge::new(|_: &State, _: &Config| Ok(vec![to("ghost")]));
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .edge(nid("a"), edge)
        .build();
    assert!(graph.is_ok());
}

#[test]
fn given_declared_end_labels_when_build_then_not_checked() {
    let edge = FnEdge::new(|_: &State, _: &Config| Ok(vec![end("done")]))
        .targets([end("done"), end("never_returned")]);
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .edge(nid("a"), edge)
        .build();
    assert!(graph.is_ok());
}

#[test]
fn given_a_failing_node_check_and_an_edge_to_a_missing_node_when_build_then_invalid_node_first() {
    let map = Map {
        list: Key::new("item").unwrap(),
        item: Key::new("item").unwrap(),
        body: Box::new(noop()),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    let result = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .map(nid("m"), map)
        .edge(nid("a"), Always(to("ghost")))
        .edge(nid("m"), Always(end("done")))
        .build();
    assert!(matches!(result, Err(GraphError::InvalidNode { node, .. }) if node == nid("m")));
}

#[test]
fn given_a_node_without_edge_and_an_edge_to_a_missing_node_when_build_then_node_without_edge_first()
{
    let result = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .node(nid("b"), noop())
        .edge(nid("a"), Always(to("ghost")))
        .build();
    assert!(matches!(result, Err(GraphError::NodeWithoutEdge { id }) if id == nid("b")));
}

#[test]
fn given_an_edge_to_an_unknown_node_when_displayed_then_it_names_the_node_and_the_target() {
    let unknown = GraphError::EdgeToUnknownNode {
        node: nid("a"),
        target: nid("ghost"),
    };
    assert_eq!(
        unknown.to_string(),
        "the edge of node a declares node ghost, which is not in the graph"
    );
}
