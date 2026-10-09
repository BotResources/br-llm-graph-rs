use super::test_support::*;
use crate::error::GraphError;
use crate::graph::subgraph::{Input, SubGraph};
use crate::graph::{Always, Context, GraphBuilder, Map, Node, NodeFuture};
use crate::state::{Config, Schema, State};

/// A call whose `text` input is left unmapped.
fn wrong_call() -> SubGraph {
    SubGraph::call(refine_graph()).config(key("rounds"), Input::Config(key("rounds")))
}

/// A user-written node that wraps a call and forwards `check` to it.
struct Wrapped(SubGraph);

impl Node for Wrapped {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        self.0.run(state, config, ctx)
    }

    fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        self.0.check(schema)
    }
}

/// A user-written node that does not override `check`.
struct Plain(SubGraph);

impl Node for Plain {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        self.0.run(state, config, ctx)
    }
}

fn single(
    register: impl FnOnce(GraphBuilder) -> GraphBuilder,
) -> Result<crate::graph::Graph, GraphError> {
    register(GraphBuilder::new(parent_schema()).entry(nid("ask")))
        .edge(nid("ask"), Always(end("done")))
        .build()
}

fn map_of(body: impl Node + 'static) -> Map {
    Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    }
}

fn assert_refused(result: Result<crate::graph::Graph, GraphError>) {
    match result {
        Err(GraphError::InvalidNode { node, source }) => {
            assert_eq!(node, nid("ask"));
            assert!(matches!(*source, GraphError::SubGraphInputUnmapped { .. }));
        }
        Err(other) => panic!("expected the node to be refused, got {other}"),
        Ok(_) => panic!("expected the node to be refused"),
    }
}

#[test]
fn given_a_wrong_call_when_registered_by_any_method_then_the_build_refuses_it() {
    assert_refused(single(|b| b.node(nid("ask"), wrong_call())));
    assert_refused(single(|b| b.join(nid("ask"), wrong_call())));
    assert_refused(single(|b| b.subgraph(nid("ask"), wrong_call())));
    assert_refused(single(|b| b.map(nid("ask"), map_of(wrong_call()))));
    assert_refused(single(|b| b.node(nid("ask"), map_of(wrong_call()))));
}

#[test]
fn given_a_wrapper_that_forwards_check_to_its_call_when_built_then_the_call_is_checked() {
    assert_refused(single(|b| b.node(nid("ask"), Wrapped(wrong_call()))));
    assert_refused(single(|b| b.map(nid("ask"), map_of(Wrapped(wrong_call())))));
    assert!(single(|b| b.node(nid("ask"), Wrapped(refine_call()))).is_ok());
}

#[test]
fn given_a_node_that_does_not_override_check_when_built_then_it_is_accepted() {
    assert!(single(|b| b.node(nid("ask"), Plain(wrong_call()))).is_ok());
    assert!(single(|b| b.map(nid("ask"), map_of(Plain(wrong_call())))).is_ok());
}

#[test]
fn given_a_map_with_keys_that_do_not_fit_when_built_then_the_map_is_refused_before_its_body() {
    let map = Map {
        list: key("question"),
        item: key("item"),
        body: Box::new(wrong_call()),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    let error = crate::testkit::refusal(single(|b| b.map(nid("ask"), map)));
    assert!(matches!(error, GraphError::MapKeyMismatch { list, .. } if list == key("question")));
}
