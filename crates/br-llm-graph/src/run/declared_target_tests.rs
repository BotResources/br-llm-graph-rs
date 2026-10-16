//! A declared edge that returns a target it did not declare fails the run
//! (`UndeclaredTarget`), before any target runs, like the other edge
//! refusals.

use crate::error::GraphError;
use crate::graph::{Always, Context, Edge, FnEdge, GraphBuilder, Input, Output, SubGraph, Target};
use crate::run::control::control;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State, Value};

/// An edge declaring `declared` that returns `returned`.
fn declared(declared: Vec<Target>, returned: Vec<Target>) -> impl Edge {
    FnEdge::new(move |_s: &State, _c: &Config| Ok(returned.clone())).targets(declared)
}

fn logged(state: &State) -> Vec<String> {
    state
        .list(&key("log"))
        .unwrap()
        .iter()
        .map(|value| match value {
            Value::Str(text) => text.clone(),
            _ => "?".to_owned(),
        })
        .collect()
}

async fn failed(graph: &crate::graph::Graph, ctx: &Context) -> crate::run::RunFailure {
    let (_sender, mut channel) = control();
    run(graph, &config(), base_state(), None, ctx, &mut channel)
        .await
        .err()
        .unwrap()
}

/// `start` fans out to `a` and `b`; `a`'s declared edge returns `y`, which
/// it does not declare, while `b` goes on to `c`.
fn fan_out(a_edge: impl Edge + 'static) -> crate::graph::Graph {
    GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), log_node("a"))
        .node(nid("b"), log_node("b"))
        .node(nid("c"), log_node("c"))
        .node(nid("x"), log_node("x"))
        .node(nid("y"), log_node("y"))
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), a_edge)
        .edge(nid("b"), Always(to("c")))
        .edge(nid("c"), Always(end("done")))
        .edge(nid("x"), Always(end("done")))
        .edge(nid("y"), Always(end("done")))
        .build()
        .unwrap()
}

#[tokio::test]
async fn given_a_declared_edge_returning_an_undeclared_node_when_run_then_undeclared_target() {
    let graph = fan_out(declared(vec![to("x")], vec![to("y")]));
    let failure = failed(&graph, &ctx()).await;
    let GraphError::UndeclaredTarget { node, target } = failure.error else {
        panic!("expected UndeclaredTarget, got {:?}", failure.error);
    };
    assert_eq!((node, target), (nid("a"), to("y")));
}

#[tokio::test]
async fn given_an_undeclared_target_when_run_then_no_target_runs_and_the_superstep_is_not_applied()
{
    let graph = fan_out(declared(vec![to("x")], vec![to("y")]));
    let failure = failed(&graph, &ctx()).await;
    assert!(matches!(failure.error, GraphError::UndeclaredTarget { .. }));
    // Neither `y` nor `c`, which `b`'s edge named, ran; nothing of the
    // superstep of `a` and `b` reached the state.
    assert!(logged(&failure.checkpoint.state).is_empty());
    assert_eq!(failure.checkpoint.cursor.active, vec![nid("a"), nid("b")]);
    let pending: Vec<String> = failure
        .checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    assert_eq!(pending, vec!["a".to_owned(), "b".to_owned()]);
}

#[tokio::test]
async fn given_the_checkpoint_of_an_undeclared_target_when_resumed_then_it_fails_again() {
    let graph = fan_out(declared(vec![to("x")], vec![to("y")]));
    let failure = failed(&graph, &ctx()).await;
    let checkpoint = failure.checkpoint;
    let (_sender, mut channel) = control();
    let again = run(
        &graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &ctx().with_pending(checkpoint.pending),
        &mut channel,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(again.error, GraphError::UndeclaredTarget { .. }));
    assert!(logged(&again.checkpoint.state).is_empty());
}

#[tokio::test]
async fn given_a_declared_edge_returning_an_undeclared_end_label_when_run_then_undeclared_target() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), declared(vec![end("done")], vec![end("other")]))
        .build()
        .unwrap();
    let failure = failed(&graph, &ctx()).await;
    let GraphError::UndeclaredTarget { node, target } = failure.error else {
        panic!("expected UndeclaredTarget, got {:?}", failure.error);
    };
    assert_eq!((node, target), (nid("a"), end("other")));
}

#[tokio::test]
async fn given_a_declared_edge_returning_an_unknown_node_when_run_then_undeclared_not_unknown() {
    let graph = fan_out(declared(vec![to("x")], vec![to("ghost")]));
    let failure = failed(&graph, &ctx()).await;
    let GraphError::UndeclaredTarget { node, target } = failure.error else {
        panic!("expected UndeclaredTarget, got {:?}", failure.error);
    };
    assert_eq!((node, target), (nid("a"), to("ghost")));
}

#[tokio::test]
async fn given_an_undeclared_edge_returning_an_unknown_node_when_run_then_unknown_node() {
    let graph = fan_out(edge(vec![to("ghost")]));
    let failure = failed(&graph, &ctx()).await;
    assert!(matches!(failure.error, GraphError::UnknownNode { id } if id == nid("ghost")));
}

#[tokio::test]
async fn given_two_declared_edges_refusing_in_one_superstep_when_run_then_the_first_registered_wins()
 {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("b"), noop_node())
        .node(nid("a"), noop_node())
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), declared(vec![end("done")], vec![end("from_a")]))
        .edge(nid("b"), declared(vec![end("done")], vec![end("from_b")]))
        .build()
        .unwrap();
    let failure = failed(&graph, &ctx()).await;
    let GraphError::UndeclaredTarget { node, target } = failure.error else {
        panic!("expected UndeclaredTarget, got {:?}", failure.error);
    };
    assert_eq!((node, target), (nid("b"), end("from_b")));
}

#[tokio::test]
async fn given_a_declared_edge_returning_no_target_when_run_then_empty_edge_first() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), declared(vec![end("done")], Vec::new()))
        .build()
        .unwrap();
    let failure = failed(&graph, &ctx()).await;
    assert!(matches!(failure.error, GraphError::EmptyEdge { node } if node == nid("a")));
}

#[tokio::test]
async fn given_a_declared_edge_returning_an_error_when_run_then_that_error_first() {
    let erring =
        FnEdge::new(|_s: &State, _c: &Config| Err(GraphError::MissingEntry)).targets([end("done")]);
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), erring)
        .build()
        .unwrap();
    let failure = failed(&graph, &ctx()).await;
    assert!(matches!(failure.error, GraphError::MissingEntry));
}

#[tokio::test]
async fn given_a_declared_edge_returning_declared_targets_when_run_then_the_run_goes_on() {
    let graph = fan_out(declared(vec![to("x"), to("y")], vec![to("y")]));
    let (_sender, mut channel) = control();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|failure| failure.error)
        .unwrap();
    let crate::run::Outcome::Finished { state, end } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "done");
    // The batches of a superstep are applied in registration order: `c`
    // before `y`.
    assert_eq!(logged(&state), vec!["a", "b", "c", "y"]);
}

#[tokio::test]
async fn given_an_undeclared_target_in_a_called_graph_when_run_then_it_is_the_source_of_the_call_failure()
 {
    let child = GraphBuilder::new(schema())
        .entry(nid("inner"))
        .node(nid("inner"), noop_node())
        .edge(
            nid("inner"),
            declared(vec![end("done")], vec![end("other")]),
        )
        .input(key("count"))
        .output(key("count"))
        .build()
        .unwrap();
    let call = SubGraph::call(std::sync::Arc::new(child))
        .input(key("count"), Input::From(key("count")))
        .output(key("count"), Output::Set(key("count")));
    let graph = GraphBuilder::new(schema())
        .entry(nid("call"))
        .subgraph(nid("call"), call)
        .edge(nid("call"), Always(end("done")))
        .build()
        .unwrap();
    let failure = failed(&graph, &ctx()).await;
    let GraphError::NodeFailed {
        node,
        source: crate::error::NodeFault::Returned(error),
    } = failure.error
    else {
        panic!("expected the call to fail");
    };
    assert_eq!(node, nid("call"));
    let Some(GraphError::SubGraphFailed { source }) = error.downcast_ref::<GraphError>() else {
        panic!("expected SubGraphFailed");
    };
    assert!(matches!(
        source.as_ref(),
        GraphError::UndeclaredTarget { node, target } if *node == nid("inner") && *target == end("other")
    ));
}

#[test]
fn given_an_undeclared_target_when_displayed_then_it_names_the_node_and_the_target_as_edge_to_unknown_node_does()
 {
    let node = GraphError::UndeclaredTarget {
        node: nid("a"),
        target: to("ghost"),
    };
    assert_eq!(
        node.to_string(),
        "the edge of node a returned node ghost, which it does not declare"
    );
    let end_label = GraphError::UndeclaredTarget {
        node: nid("a"),
        target: end("other"),
    };
    assert_eq!(
        end_label.to_string(),
        "the edge of node a returned end other, which it does not declare"
    );
}
