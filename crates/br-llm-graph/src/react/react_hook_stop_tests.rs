//! Which calls a failing or panicking hook stops: a call already judging
//! when another hook fails goes on and runs; without a concurrency limit, a
//! hook failing at once stops the calls after it from starting; a panic
//! drops every other call of the node at once.

use std::sync::Arc;

use serde_json::json;

use crate::error::{GraphError, NodeFault};
use crate::react::react_hook_support::*;

#[tokio::test]
async fn given_a_call_judging_when_another_hook_fails_then_it_is_accepted_and_its_tool_runs() {
    // c1's hook fails on its second poll, while c2's hook is still judging.
    let gauge = Arc::new(Gauge::default());
    let failing = GuardedTool::new(
        "fetch",
        Some(Behaviour::SlowFail {
            yields: 1,
            name: "fetch",
        }),
        &gauge,
    );
    let judging = GuardedTool::new("write_file", Some(Behaviour::Slow { yields: 4 }), &gauge);
    let graph = tool_graph(vec![failing.clone(), judging.clone()], None, false);

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "fetch", json!({})),
            call("c2", "write_file", json!({})),
        ]),
    )
    .await
    .err()
    .unwrap();

    assert!(matches!(
        failure.error,
        GraphError::NodeFailed {
            source: NodeFault::Returned(_),
            ..
        }
    ));
    assert_eq!(judging.seen().len(), 1);
    assert_eq!(judging.calls(), 1, "a started call is accepted and runs");
    assert_eq!(judging.finished(), 1);
}

#[tokio::test]
async fn given_no_concurrency_limit_when_a_hook_fails_at_once_then_the_calls_after_it_do_not_start()
{
    let gauge = Arc::new(Gauge::default());
    let failing = GuardedTool::new("fetch", Some(Behaviour::Fail("fetch")), &gauge);
    let after = GuardedTool::new("write_file", Some(Behaviour::Accept), &gauge);
    let graph = tool_graph(vec![failing, after.clone()], None, false);

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "fetch", json!({ "name": "fetch" })),
            call("c2", "write_file", json!({})),
        ]),
    )
    .await
    .err()
    .unwrap();

    assert!(matches!(failure.error, GraphError::NodeFailed { .. }));
    assert!(after.seen().is_empty(), "c2 is never judged");
    assert_eq!(after.calls(), 0, "c2 never runs");
}

#[tokio::test]
async fn given_a_hook_that_panics_beside_a_running_call_when_it_judges_then_the_node_fails_as_a_panic_and_the_running_call_is_dropped()
 {
    let gauge = Arc::new(Gauge::default());
    let running = GuardedTool::new("write_file", Some(Behaviour::Accept), &gauge);
    let broken = GuardedTool::new("fetch", Some(Behaviour::Panic), &gauge);
    let graph = tool_graph(vec![running.clone(), broken], None, false);

    let failure = run_tools(
        &graph,
        pending(vec![
            call("c1", "write_file", json!({ "yields": 10 })),
            call("c2", "fetch", json!({})),
        ]),
    )
    .await
    .err()
    .unwrap();

    assert!(
        matches!(
            failure.error,
            GraphError::NodeFailed {
                ref node,
                source: NodeFault::Panic(ref message),
            } if *node == nid("tools") && message.contains("the policy hook broke")
        ),
        "{}",
        failure.error
    );
    assert_eq!(running.calls(), 1, "c1 had started");
    assert_eq!(running.finished(), 0, "the panic dropped it at once");
}
