//! A run that fails: a node that returns an error or whose updates are
//! refused, an edge naming an unknown node, the held inputs kept in the
//! failure's checkpoint, the node's error read back by the host.

use crate::error::{GraphError, NodeFault};
use crate::graph::{Always, FnNode, GraphBuilder, NodeFuture};
use crate::run::control::control;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{Config, State};
use crate::update::Update;

#[tokio::test]
async fn given_node_returns_input_on_non_conversation_key_when_run_then_not_conversation() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(
            nid("a"),
            FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
                Box::pin(async {
                    Ok(vec![Update::Input {
                        key: key("count"),
                        input: input("oops"),
                    }])
                })
            }),
        )
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    let GraphError::NodeFailed {
        source: NodeFault::Refused(inner),
        ..
    } = failure.error
    else {
        panic!("expected a refused node fault");
    };
    assert!(matches!(*inner, GraphError::NotConversation { .. }));
}

#[tokio::test]
async fn given_failing_node_beside_success_when_run_then_nothing_applied_and_the_success_pending() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("a"), set_count_node(5))
        .node(nid("b"), failing_node())
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
    assert!(matches!(
        failure.error,
        crate::error::GraphError::NodeFailed { .. }
    ));
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 0);
    let pending: Vec<String> = failure
        .checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    assert_eq!(pending, vec!["a"]);
}

#[tokio::test]
async fn given_input_queued_when_node_fails_then_input_kept_in_failure_checkpoint() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), failing_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.send(key("chat"), input("late"));
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::NodeFailed { .. }
    ));
    assert_eq!(
        failure
            .checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        1
    );
}

#[tokio::test]
async fn given_control_input_targets_non_conversation_key_when_run_then_not_conversation() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.send(key("count"), input("oops"));
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::NotConversation { .. }
    ));
}

#[tokio::test]
async fn given_edge_targets_unknown_node_alone_when_run_then_unknown_node_error() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), edge(vec![to("missing")]))
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    match failure.error {
        crate::error::GraphError::UnknownNode { id } => assert_eq!(id.as_str(), "missing"),
        other => panic!("expected UnknownNode, got {other}"),
    }
}

#[tokio::test]
async fn given_fan_out_edge_targets_one_unknown_node_when_run_then_unknown_node_error() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("start"))
        .node(nid("start"), noop_node())
        .node(nid("real"), noop_node())
        .edge(nid("start"), edge(vec![to("real"), to("typo")]))
        .edge(nid("real"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    match failure.error {
        crate::error::GraphError::UnknownNode { id } => assert_eq!(id.as_str(), "typo"),
        other => panic!("expected UnknownNode, got {other}"),
    }
}

#[derive(Debug)]
struct Refusal {
    code: &'static str,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "refused: {}", self.code)
    }
}

impl std::error::Error for Refusal {}

#[tokio::test]
async fn given_node_returns_a_typed_error_when_run_fails_then_the_host_reads_it_back() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(
            nid("a"),
            FnNode::new(|_s: &State, _c: &Config, _x: &_| -> NodeFuture<'_> {
                Box::pin(async {
                    Err(Box::new(Refusal {
                        code: "INPUT_REFUSED",
                    }) as crate::graph::NodeError)
                })
            }),
        )
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();

    let GraphError::NodeFailed {
        source: NodeFault::Returned(returned),
        ..
    } = &failure.error
    else {
        panic!("expected a node failure");
    };
    assert_eq!(
        returned.downcast_ref::<Refusal>().unwrap().code,
        "INPUT_REFUSED"
    );
    let through_sources = std::error::Error::source(&failure.error)
        .and_then(std::error::Error::source)
        .and_then(|error| error.downcast_ref::<Refusal>());
    assert!(through_sources.is_some());
    assert_eq!(
        failure.error.to_string(),
        "node a failed: returned an error: refused: INPUT_REFUSED"
    );
}
