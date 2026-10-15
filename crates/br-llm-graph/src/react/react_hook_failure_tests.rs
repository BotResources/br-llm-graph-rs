//! A hook that fails: no call that has not started is started afterwards,
//! the calls already started finish, the node fails with the first error in
//! call order of the hooks that failed, and its results are dropped.

use std::sync::Arc;

use serde_json::json;

use crate::error::{GraphError, NodeFault};
use crate::react::react_hook_support::*;
use crate::react::test_support::*;
use crate::run::RunFailure;

/// The error the tool node returned, read as the hook's `PolicyDown`.
fn policy_down(failure: &RunFailure) -> &'static str {
    let GraphError::NodeFailed {
        node,
        source: NodeFault::Returned(error),
    } = &failure.error
    else {
        panic!("expected the tool node to fail, got: {}", failure.error);
    };
    assert_eq!(node, &nid("tools"));
    error
        .downcast_ref::<PolicyDown>()
        .map(|down| down.0)
        .expect("the hook's error, as returned")
}

#[tokio::test]
async fn given_hooks_that_fail_on_two_calls_when_they_are_pending_then_the_first_in_call_order_is_returned()
 {
    // The early hook fails last in time: the error is still its own.
    let gauge = Arc::new(Gauge::default());
    let early = GuardedTool::new(
        "early",
        Some(Behaviour::SlowFail {
            yields: 5,
            name: "early",
        }),
        &gauge,
    );
    let late = GuardedTool::new("late", Some(Behaviour::Fail("late")), &gauge);
    let graph = tool_graph(vec![early.clone(), late.clone()], None, false);

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "early", json!({})),
            call("c2", "late", json!({ "name": "late" })),
        ]),
    )
    .await
    .err()
    .unwrap();

    assert_eq!(policy_down(&failure), "early");
    assert_eq!(early.calls() + late.calls(), 0);
}

#[tokio::test]
async fn given_a_hook_failing_while_another_call_runs_when_the_calls_end_then_the_running_call_finishes_and_the_first_error_in_call_order_is_returned()
 {
    // c1's hook fails slowly, c2's quickly; c3 was accepted and its tool
    // started before c2 failed.
    let gauge = Arc::new(Gauge::default());
    let early = GuardedTool::new(
        "early",
        Some(Behaviour::SlowFail {
            yields: 6,
            name: "early",
        }),
        &gauge,
    );
    let late = GuardedTool::new(
        "late",
        Some(Behaviour::SlowFail {
            yields: 1,
            name: "late",
        }),
        &gauge,
    );
    let writer = GuardedTool::new("write_file", Some(Behaviour::Accept), &gauge);
    let graph = tool_graph(
        vec![early.clone(), late.clone(), writer.clone()],
        None,
        false,
    );

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "early", json!({})),
            call("c2", "late", json!({})),
            call("c3", "write_file", json!({ "yields": 4 })),
        ]),
    )
    .await
    .err()
    .unwrap();

    assert_eq!(policy_down(&failure), "early");
    assert_eq!(writer.calls(), 1, "c3 had started");
    assert_eq!(writer.finished(), 1, "a started call runs to its end");
    let Some(br_llm_messages::Entry::Turn(turn)) = failure
        .checkpoint
        .state
        .conversation(&key("chat"))
        .unwrap()
        .entries()
        .last()
    else {
        panic!("expected the agent's turn");
    };
    assert!(
        results_of(turn).is_empty(),
        "the node's results are dropped, c3's included"
    );
}

#[tokio::test]
async fn given_a_call_queued_behind_a_failing_hook_when_the_hook_fails_then_the_queued_call_never_starts()
 {
    // R3's shape: with one call at a time, `delete_file`'s hook cannot
    // judge, and `send_email` waits for the slot.
    let gauge = Arc::new(Gauge::default());
    let delete = GuardedTool::new(
        "delete_file",
        Some(Behaviour::SlowFail {
            yields: 3,
            name: "delete_file",
        }),
        &gauge,
    );
    let email = GuardedTool::new("send_email", Some(Behaviour::Accept), &gauge);
    let graph = tool_graph(vec![delete.clone(), email.clone()], Some(1), false);

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "delete_file", json!({})),
            call("c2", "send_email", json!({})),
        ]),
    )
    .await
    .err()
    .unwrap();

    assert_eq!(policy_down(&failure), "delete_file");
    assert!(email.seen().is_empty(), "the queued call is never judged");
    assert_eq!(email.calls(), 0, "the queued call is never run");
    assert_eq!(delete.calls(), 0);
}

#[tokio::test]
async fn given_a_queued_call_and_no_hook_failure_when_the_slot_frees_then_it_runs() {
    // Positive control of the test above: the same shape, `delete_file`
    // accepted.
    let gauge = Arc::new(Gauge::default());
    let delete = GuardedTool::new("delete_file", Some(Behaviour::Slow { yields: 3 }), &gauge);
    let email = GuardedTool::new("send_email", Some(Behaviour::Accept), &gauge);
    let graph = tool_graph(vec![delete.clone(), email.clone()], Some(1), false);

    let results = answered(
        run_tools(
            &graph,
            pending(vec![
                call("c1", "delete_file", json!({})),
                call("c2", "send_email", json!({})),
            ]),
        )
        .await,
    );

    assert_eq!(results, [ran("delete_file"), ran("send_email")]);
    assert_eq!(email.seen().len(), 1);
    assert_eq!(email.calls(), 1);
}
