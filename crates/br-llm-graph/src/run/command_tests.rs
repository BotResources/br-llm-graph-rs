//! The commands of the control channel: an input held and applied after the
//! superstep's updates, pause honoured at its end, resume outside a pause
//! ignored, a checkpoint naming an unknown node refused.

use crate::graph::{Always, GraphBuilder};
use crate::run::control::control;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;

#[tokio::test]
async fn given_input_queued_when_run_then_drained_at_boundary() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .node(nid("b"), noop_node())
        .edge(nid("a"), Always(to("b")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.send(key("chat"), input("hello"));
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.conversation(&key("chat")).unwrap().entries().len(), 1);
}

#[tokio::test]
async fn given_pause_when_run_then_paused_at_boundary_without_losing_work() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), set_count_node(7))
        .node(nid("b"), noop_node())
        .edge(nid("a"), Always(to("b")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.pause();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Paused { checkpoint } = outcome else {
        panic!("expected paused");
    };
    assert_eq!(checkpoint.state.int(&key("count")).unwrap(), 7);
    assert_eq!(
        checkpoint
            .cursor
            .active
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
}

#[tokio::test]
async fn given_resume_without_pause_when_run_then_ignored() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.resume();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    assert!(matches!(outcome, Outcome::Finished { .. }));
}

#[tokio::test]
async fn given_input_and_pause_same_superstep_when_run_then_paused_with_input_applied() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .node(nid("b"), noop_node())
        .edge(nid("a"), Always(to("b")))
        .edge(nid("b"), Always(end("done")))
        .build()
        .unwrap();
    let (sender, mut channel) = control();
    sender.send(key("chat"), input("hello"));
    sender.pause();
    let outcome = run(&graph, &config(), base_state(), None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Paused { checkpoint } = outcome else {
        panic!("expected paused");
    };
    assert_eq!(
        checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        1
    );
    assert_eq!(
        checkpoint
            .cursor
            .active
            .iter()
            .map(|n| n.as_str())
            .collect::<Vec<_>>(),
        vec!["b"]
    );
}

#[tokio::test]
async fn given_checkpoint_with_unknown_node_when_validated_then_refused() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), Always(end("done")))
        .build()
        .unwrap();
    let cursor = crate::run::Cursor::new(vec![nid("ghost")], Vec::new());
    assert!(matches!(
        graph.validate_cursor(&cursor),
        Err(crate::error::GraphError::UnknownNode { .. })
    ));
}
