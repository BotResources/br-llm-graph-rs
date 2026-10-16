//! A hook error, then a resume from the failure's checkpoint: durable, the
//! call is answered as interrupted when the agent enters its thread again,
//! never judged again; in memory, the call restarts from `opening`, so the
//! model is asked again and its new call is judged.

use std::sync::Arc;

use br_llm_messages::Entry;
use serde_json::json;

use crate::error::{GraphError, NodeFault};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::*;
use crate::react::react_hook_support::*;
use crate::react::test_support::*;
use crate::run::{RunFailure, control, run};
use crate::state::{Kind, Value};
use crate::testkit::{INTERRUPTED, TestStore};

/// A tool `echo` whose hook fails its first judgment and accepts the others.
fn guarded_once() -> Arc<GuardedTool> {
    GuardedTool::new(
        "echo",
        Some(Behaviour::SlowFailOnce {
            yields: 0,
            name: "echo",
        }),
        &Arc::new(Gauge::default()),
    )
}

/// That `failure` is the hook's error, failing the tool node.
fn failed_by_the_hook(failure: &RunFailure) {
    match &failure.error {
        GraphError::NodeFailed {
            node,
            source: NodeFault::Returned(error),
        } => {
            assert_eq!(node.as_str(), "tools");
            assert!(error.downcast_ref::<PolicyDown>().is_some(), "{error}");
        }
        other => panic!("expected the hook to fail the tool node, got: {other}"),
    }
}

#[tokio::test]
async fn given_a_durable_agent_failed_by_a_hook_when_resumed_then_the_call_is_answered_as_interrupted_never_judged_again()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("final"),
    ]);
    let tool = guarded_once();
    let graph = ReactAgent {
        tools: vec![tool.clone()],
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let failed = failure(call_durable(&host, &graph, opening(&[said("i1", "hi")]), "", "r1").await);
    failed_by_the_hook(&failed);
    assert_eq!((tool.seen().len(), tool.calls()), (1, 0));

    let (_sender, mut channel) = control();
    let resumed = run(
        &graph,
        &agent_config(&graph),
        failed.checkpoint.state,
        Some(failed.checkpoint.cursor),
        &host.ctx("r1"),
        &mut channel,
    )
    .await;

    let state = finished(resumed);
    assert_eq!(state.str(&key("answer")).unwrap(), "final");
    assert_eq!(
        (tool.seen().len(), tool.calls()),
        (1, 0),
        "at most once: judged once, never run"
    );
    assert_eq!(client.calls(), 2, "the model reads the interrupted result");
    let thread = store.view(&nth_thread(1)).unwrap().conversation;
    let first = thread
        .entries()
        .iter()
        .find_map(|entry| match entry {
            Entry::Turn(turn) => Some(turn.clone()),
            Entry::UserInput(_) => None,
        })
        .unwrap();
    assert_eq!(
        results_of(&first),
        [("echo".to_owned(), true, INTERRUPTED.to_owned())]
    );
}

#[tokio::test]
async fn given_an_agent_in_memory_failed_by_a_hook_when_resumed_then_the_call_restarts_from_its_opening_and_the_new_call_is_judged()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c1", "echo", json!({})),
        text_step("final"),
    ]);
    let tool = guarded_once();
    let graph = ReactAgent {
        tools: vec![tool.clone()],
        inputs: vec![(key("log"), Kind::list(Kind::Str))],
        ..agent(&client)
    }
    .graph()
    .unwrap();
    let state = graph
        .start_state([
            (key("opening"), Value::conversation(question("hi"))),
            (key("log"), Value::list(Vec::new())),
        ])
        .unwrap();
    let (_sender, mut channel) = control();
    let failed = run(
        &graph,
        &agent_config(&graph),
        state,
        None,
        &ctx(),
        &mut channel,
    )
    .await
    .err()
    .unwrap();
    failed_by_the_hook(&failed);
    assert_eq!(client.calls(), 1);

    let (_sender, mut channel) = control();
    let resumed = run(
        &graph,
        &agent_config(&graph),
        failed.checkpoint.state,
        Some(failed.checkpoint.cursor),
        &ctx(),
        &mut channel,
    )
    .await;

    let state = finished(resumed);
    assert_eq!(state.str(&key("answer")).unwrap(), "final");
    assert_eq!(
        client.calls(),
        3,
        "the model is asked again from the opening"
    );
    assert_eq!((tool.seen().len(), tool.calls()), (2, 1));
}
