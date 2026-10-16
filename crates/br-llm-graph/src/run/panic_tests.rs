//! A node that panics, whether its run call panics before giving its
//! future or the future does: a failure of that node, the state intact.

use crate::error::{GraphError, NodeFault};
use crate::graph::{Always, Context, GraphBuilder, NodeFuture};
use crate::run::control::control;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State};

#[tokio::test]
async fn given_panicking_node_when_run_then_node_failed_with_state_intact() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("p"))
        .node(nid("p"), panicking_node())
        .edge(nid("p"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    match failure.error {
        crate::error::GraphError::NodeFailed { source, .. } => {
            assert!(matches!(source, crate::error::NodeFault::Panic(_)));
        }
        other => panic!("expected NodeFailed, got {other}"),
    }
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 0);
}

struct PanicsOnCall;

impl crate::graph::Node for PanicsOnCall {
    fn run<'a>(&'a self, _s: &'a State, _c: &'a Config, _x: &'a Context) -> NodeFuture<'a> {
        panic!("panics before its future")
    }
}

#[tokio::test]
async fn given_node_panics_while_its_run_starts_when_run_then_the_panic_is_a_node_failure() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("p"))
        .node(nid("p"), PanicsOnCall)
        .edge(nid("p"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    let GraphError::NodeFailed {
        node,
        source: NodeFault::Panic(message),
    } = failure.error
    else {
        panic!("expected a panic fault");
    };
    assert_eq!(node.as_str(), "p");
    assert_eq!(message, "panics before its future");
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 0);
}

#[tokio::test]
async fn given_panicking_node_beside_success_when_run_then_nothing_applied_and_panic_reported() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), set_count_node(5))
        .node(nid("b"), panicking_node())
        .edge(nid("start"), edge(vec![to("a"), to("b")]))
        .edge(nid("a"), Always(end("done")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    match failure.error {
        crate::error::GraphError::NodeFailed { source, .. } => {
            assert!(matches!(source, crate::error::NodeFault::Panic(_)));
        }
        other => panic!("expected NodeFailed, got {other}"),
    }
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 0);
    let pending: Vec<String> = failure
        .checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    assert_eq!(pending, vec!["a"]);
}
