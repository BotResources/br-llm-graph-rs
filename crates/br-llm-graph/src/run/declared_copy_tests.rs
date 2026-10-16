//! The declared targets of an edge are taken once, when the graph is built:
//! the run checks that copy, and the description writes it.

use std::sync::atomic::{AtomicUsize, Ordering};

use crate::error::GraphError;
use crate::graph::{Always, Edge, FnEdge, Graph, GraphBuilder, Target};
use crate::run::control::control;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State};

/// A host's edge whose declared targets change after the first time they
/// are asked: `first`, then `later`. It returns `returned`.
struct Shifting {
    asked: AtomicUsize,
    first: Vec<Target>,
    later: Vec<Target>,
    returned: Vec<Target>,
}

impl Edge for Shifting {
    fn next(&self, _state: &State, _config: &Config) -> Result<Vec<Target>, GraphError> {
        Ok(self.returned.clone())
    }

    fn declared(&self) -> Option<&[Target]> {
        if self.asked.fetch_add(1, Ordering::SeqCst) == 0 {
            Some(&self.first)
        } else {
            Some(&self.later)
        }
    }
}

/// `a` goes to `y` through `edge`; `x` and `y` end the run.
fn graph(edge: impl Edge + 'static) -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .node(nid("x"), noop_node())
        .node(nid("y"), noop_node())
        .edge(nid("a"), edge)
        .edge(nid("x"), Always(end("done")))
        .edge(nid("y"), Always(end("done")))
        .build()
        .unwrap()
}

fn shifting() -> Shifting {
    Shifting {
        asked: AtomicUsize::new(0),
        first: vec![to("x")],
        later: vec![to("y")],
        returned: vec![to("y")],
    }
}

async fn error_of(graph: &Graph) -> Option<GraphError> {
    let (_sender, mut channel) = control();
    run(graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .map(|failure| failure.error)
}

#[tokio::test]
async fn given_declared_targets_that_change_after_the_build_when_run_then_the_build_copy_is_checked()
 {
    let graph = graph(shifting());
    let _ = graph.describe();
    assert!(matches!(
        error_of(&graph).await,
        Some(GraphError::UndeclaredTarget { node, target }) if node == nid("a") && target == to("y")
    ));
}

#[test]
fn given_declared_targets_that_change_after_the_build_when_described_then_the_build_copy_is_written()
 {
    let graph = graph(shifting());
    let json = serde_json::to_value(graph.describe()).unwrap();
    let again = serde_json::to_value(graph.describe()).unwrap();
    assert_eq!(
        json["nodes"][0]["edge"],
        serde_json::json!({"targets": [{"node": "x"}]})
    );
    assert_eq!(json, again);
}

/// A host's edge that declares a target twice.
struct Repeating(Vec<Target>);

impl Edge for Repeating {
    fn next(&self, _state: &State, _config: &Config) -> Result<Vec<Target>, GraphError> {
        Ok(vec![to("x")])
    }

    fn declared(&self) -> Option<&[Target]> {
        Some(&self.0)
    }
}

#[tokio::test]
async fn given_a_target_declared_twice_when_built_then_it_is_kept_once_at_its_first_place() {
    // `x` again after `done`: kept at its first place, `[x, done]`; kept at
    // its last place it would read `[done, x]`.
    let graph = graph(Repeating(vec![to("x"), end("done"), to("x")]));
    let json = serde_json::to_value(graph.describe()).unwrap();
    assert_eq!(
        json["nodes"][0]["edge"],
        serde_json::json!({"targets": [{"node": "x"}, {"end": "done"}]})
    );
    assert!(error_of(&graph).await.is_none());
}

#[tokio::test]
async fn given_several_undeclared_targets_when_run_then_the_first_returned_is_named() {
    let edge = FnEdge::new(|_s: &State, _c: &Config| Ok(vec![to("y"), to("x"), end("other")]))
        .targets([to("x")]);
    assert!(matches!(
        error_of(&graph(edge)).await,
        Some(GraphError::UndeclaredTarget { node, target }) if node == nid("a") && target == to("y")
    ));
}
