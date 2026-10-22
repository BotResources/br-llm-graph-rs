//! A durable round split between tool nodes, where the first answered a
//! call of a tool no node runs and another failed: the round is not written,
//! and the resumed agent finds its turn open, fails it so the store repairs
//! it, and goes on in a thread that still renders.

use std::sync::Arc;

use br_llm_messages::Entry;

use crate::error::{GraphError, NodeFault};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::{agent_config, finished, nid};
use crate::react::react_durable_support::*;
use crate::react::react_hook_support::{Behaviour, Gauge, GuardedTool, PolicyDown};
use crate::react::test_support::*;
use crate::run::{control, run};
use crate::testkit::{INTERRUPTED, TestStore};

#[tokio::test]
async fn given_a_split_round_failed_after_its_first_node_answered_an_unavailable_call_when_resumed_then_the_thread_is_repaired_and_renders()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![calls_step(&["ghost", "echo"]), text_step("final")]);
    // `echo`'s hook fails its first judgment: its node fails the round.
    let echo = GuardedTool::new(
        "echo",
        Some(Behaviour::SlowFailOnce {
            yields: 0,
            name: "echo",
        }),
        &Arc::new(Gauge::default()),
    );
    let graph = ReactAgent {
        tools: vec![Arc::new(TallyTool), echo.clone()],
        tool_nodes: vec![
            (nid("talliers"), vec![Arc::new(TallyTool) as _]),
            (nid("echoers"), vec![echo.clone() as _]),
        ],
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();

    let failed = failure(call_durable(&host, &graph, opening(&[said("i1", "hi")]), "", "r1").await);
    match &failed.error {
        GraphError::NodeFailed {
            node,
            source: NodeFault::Returned(error),
        } => {
            assert_eq!(node.as_str(), "echoers");
            assert!(error.downcast_ref::<PolicyDown>().is_some(), "{error}");
        }
        other => panic!("expected the hook to fail the echoers, got: {other}"),
    }
    assert!(
        !writes(&store)
            .iter()
            .any(|line| line.starts_with("results ")),
        "the round is not written: {:?}",
        writes(&store)
    );

    let (_sender, mut channel) = control();
    let state = finished(
        run(
            &graph,
            &agent_config(&graph),
            failed.checkpoint.state,
            Some(failed.checkpoint.cursor),
            &host.ctx("r1"),
            &mut channel,
        )
        .await,
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "final");
    let thread = store.view(&nth_thread(1)).unwrap().conversation;
    assert!(
        crate::react::wire(&thread, &author()).is_ok(),
        "the thread renders"
    );
    let repaired = thread
        .entries()
        .iter()
        .find_map(|entry| match entry {
            Entry::Turn(turn) => Some(turn.clone()),
            Entry::UserInput(_) => None,
        })
        .unwrap();
    assert_eq!(
        results_of(&repaired),
        [
            ("ghost".to_owned(), true, INTERRUPTED.to_owned()),
            ("echo".to_owned(), true, INTERRUPTED.to_owned()),
        ]
    );
    assert_eq!((echo.seen().len(), echo.calls()), (1, 0));
}
