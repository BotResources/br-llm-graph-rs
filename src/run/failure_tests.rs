use std::sync::Arc;

use br_llm_messages::{Text, UserBlock, UserInput, UserSource};

use crate::graph::{Always, Context, GraphBuilder};
use crate::observe::NoopObserver;
use crate::run::inbox::channel;
use crate::run::runner::run;
use crate::run::test_support::*;

fn ctx() -> Context {
    Context::new(
        Arc::new(NoopObserver),
        Arc::new(crate::testkit::SeqIds::new()),
    )
}

fn input(body: &str) -> UserInput {
    UserInput::new(
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

#[tokio::test]
async fn given_failing_node_beside_success_when_run_then_success_kept_failure_reported() {
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
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::NodeFailed { .. }
    ));
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 5);
}

#[tokio::test]
async fn given_panicking_node_when_run_then_node_failed_with_state_intact() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("p"))
        .node(nid("p"), panicking_node())
        .edge(nid("p"), Always(end("done")))
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
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

#[tokio::test]
async fn given_panicking_node_beside_success_when_run_then_success_kept_and_panic_reported() {
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
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    match failure.error {
        crate::error::GraphError::NodeFailed { source, .. } => {
            assert!(matches!(source, crate::error::NodeFault::Panic(_)));
        }
        other => panic!("expected NodeFailed, got {other}"),
    }
    assert_eq!(failure.checkpoint.state.int(&key("count")).unwrap(), 5);
}

#[tokio::test]
async fn given_input_queued_when_node_fails_then_input_kept_in_failure_checkpoint() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), failing_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut inbox) = channel();
    sender.send(key("chat"), input("late"));
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
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
async fn given_inbox_input_targets_non_conversation_key_when_run_then_not_conversation() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut inbox) = channel();
    sender.send(key("count"), input("oops"));
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
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
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
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
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), base_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    match failure.error {
        crate::error::GraphError::UnknownNode { id } => assert_eq!(id.as_str(), "typo"),
        other => panic!("expected UnknownNode, got {other}"),
    }
}
