//! Hooks: the judgment of each tool call before its tool runs, made after
//! the enabled-key check. An accepted call runs, a refused one is answered
//! with an error result holding the reason, a hook error fails the node.

use std::sync::Arc;

use br_llm_messages::Entry;
use serde_json::json;

use crate::error::{GraphError, NodeFault};
use crate::react::react_hook_support::*;
use crate::react::test_support::*;

#[tokio::test]
async fn given_a_hook_that_accepts_when_the_call_is_pending_then_the_tool_runs() {
    let gauge = Arc::new(Gauge::default());
    let tool = GuardedTool::new("write_file", Some(Behaviour::Accept), &gauge);
    let graph = tool_graph(vec![tool.clone()], None, false);

    let results = answered(
        run_tools(
            &graph,
            pending(vec![call("c1", "write_file", json!({ "path": "notes/a" }))]),
        )
        .await,
    );

    assert_eq!(results, [ran("write_file")]);
    assert_eq!(tool.calls(), 1);
    assert_eq!(
        tool.seen(),
        [(
            "c1".to_owned(),
            json!({ "path": "notes/a" }),
            "1".to_owned(),
            "You are a helpful agent.".to_owned()
        )],
        "the hook is called with the call's id, its arguments, the state and the configuration"
    );
}

#[tokio::test]
async fn given_a_hook_that_refuses_when_the_call_is_pending_then_an_error_result_holds_the_reason_and_the_tool_does_not_run()
 {
    let gauge = Arc::new(Gauge::default());
    let guarded = GuardedTool::new(
        "write_file",
        Some(Behaviour::Refuse("writes outside notes/ are refused")),
        &gauge,
    );
    let free = GuardedTool::new("echo", None, &gauge);
    let graph = tool_graph(vec![guarded.clone(), free.clone()], None, false);

    let results = answered(
        run_tools(
            &graph,
            pending(vec![
                call("c1", "write_file", json!({ "path": "/etc/passwd" })),
                call("c2", "echo", json!({})),
            ]),
        )
        .await,
    );

    assert_eq!(
        results,
        [
            refused("write_file", "writes outside notes/ are refused"),
            ran("echo"),
        ],
        "the turn goes on: the other call runs"
    );
    assert_eq!(guarded.calls(), 0);
    assert_eq!(free.calls(), 1, "a tool without a hook is run as is");
}

#[tokio::test]
async fn given_a_hook_that_refuses_with_a_blank_reason_when_the_call_is_pending_then_tool_call_refused()
 {
    for reason in ["", " \n"] {
        let gauge = Arc::new(Gauge::default());
        let tool = GuardedTool::new("write_file", Some(Behaviour::Refuse(reason)), &gauge);
        let graph = tool_graph(vec![tool.clone()], None, false);

        let results =
            answered(run_tools(&graph, pending(vec![call("c1", "write_file", json!({}))])).await);

        assert_eq!(
            results,
            [refused("write_file", "tool call refused")],
            "{reason:?}"
        );
        assert_eq!(tool.calls(), 0);
    }
}

#[tokio::test]
async fn given_a_hook_that_fails_when_the_call_is_pending_then_the_node_fails_with_the_hook_error_and_the_call_is_not_run()
 {
    let gauge = Arc::new(Gauge::default());
    let tool = GuardedTool::new("write_file", Some(Behaviour::Fail("second")), &gauge);
    let graph = tool_graph(vec![tool.clone()], None, false);

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "write_file", json!({ "name": "first" })),
            call("c2", "write_file", json!({ "name": "second" })),
        ]),
    )
    .await
    .err()
    .unwrap();

    let GraphError::NodeFailed {
        node,
        source: NodeFault::Returned(error),
    } = &failure.error
    else {
        panic!("expected the tool node to fail, got: {}", failure.error);
    };
    assert_eq!(node, &nid("tools"));
    assert_eq!(
        error.downcast_ref::<PolicyDown>().map(|down| down.0),
        Some("second"),
        "the hook's error reaches the host as it returned it"
    );
    assert_eq!(tool.calls(), 1, "the refused-by-error call is not run");
    let checkpoint = &failure.checkpoint.state;
    let Some(Entry::Turn(turn)) = checkpoint
        .conversation(&key("chat"))
        .unwrap()
        .entries()
        .last()
    else {
        panic!("expected the agent's turn");
    };
    assert!(results_of(turn).is_empty(), "no update of the failed node");
}

#[tokio::test]
async fn given_hooks_that_fail_on_two_calls_when_they_are_pending_then_the_first_in_call_order_is_returned()
 {
    let gauge = Arc::new(Gauge::default());
    let late = GuardedTool::new("late", Some(Behaviour::Fail("late")), &gauge);
    let early = GuardedTool::new("early", Some(Behaviour::Fail("early")), &gauge);
    let graph = tool_graph(vec![late.clone(), early.clone()], None, false);

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "early", json!({ "name": "early" })),
            call("c2", "late", json!({ "name": "late" })),
        ]),
    )
    .await
    .err()
    .unwrap();

    let GraphError::NodeFailed {
        source: NodeFault::Returned(error),
        ..
    } = &failure.error
    else {
        panic!("expected the tool node to fail");
    };
    assert_eq!(
        error.downcast_ref::<PolicyDown>().map(|down| down.0),
        Some("early")
    );
    assert_eq!(late.calls() + early.calls(), 0);
}

#[tokio::test]
async fn given_a_call_naming_a_tool_that_is_not_enabled_when_it_is_pending_then_its_hook_is_not_called()
 {
    let gauge = Arc::new(Gauge::default());
    let tool = GuardedTool::new("write_file", Some(Behaviour::Accept), &gauge);
    let graph = tool_graph(vec![tool.clone()], None, true);

    let results =
        answered(run_tools(&graph, pending(vec![call("c1", "write_file", json!({}))])).await);

    assert_eq!(
        results,
        [refused(
            "write_file",
            "Tool write_file is not enabled for this run: this call was not executed."
        )]
    );
    assert!(
        tool.seen().is_empty(),
        "the enabled key is checked first: no judgment"
    );
    assert_eq!(tool.calls(), 0);
}
