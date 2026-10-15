//! The ReAct loop's nodes run: tool results and their updates, the enabled
//! key, and the model-call step on a turn that awaits results.

use std::sync::Arc;

use br_llm_clients_core::ToolSpec;
use br_llm_messages::{AssistantBlock, Entry, Step, StopReason, ToolCall, ToolCallId, ToolName};
use serde_json::json;

use crate::graph::{Always, GraphBuilder, Target};
use crate::react::react_node_support::*;
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::run::{Outcome, control, run};
use crate::state::{State, Value};
use crate::value::EndLabel;

#[tokio::test]
async fn given_arguments_that_fail_to_parse_when_tool_runs_then_error_result() {
    let step = tool_call_step("c1", "strict", json!({ "n": "not a number" }));
    let state = run_tool_node(state_with_call(step), vec![Arc::new(StrictTool)]).await;
    assert_eq!(first_result_is_error(&state), Some(true));
}

#[tokio::test]
async fn given_tool_returning_updates_when_run_then_updates_applied() {
    let step = tool_call_step("c1", "tally", json!({}));
    let state = run_tool_node(state_with_call(step), vec![Arc::new(TallyTool)]).await;
    assert_eq!(state.list(&key("log")).unwrap().len(), 1);
    assert_eq!(first_result_is_error(&state), Some(false));
}

#[tokio::test]
async fn given_a_tool_node_with_an_enabled_key_when_calls_name_a_tool_it_does_not_hold_then_that_call_is_refused_unrun()
 {
    let writer = Arc::new(WriteFileTool::default());
    let mut state = state_with_call(calls_step(&["write_file", "echo"]));
    state
        .apply_batch(&[crate::update::Update::Set {
            key: key("enabled"),
            value: Value::list(vec![Value::str("echo")]),
        }])
        .unwrap();
    let graph = GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(
            nid("tools"),
            tool_node(
                vec![Arc::new(EchoTool), writer.clone()],
                None,
                Some(key("enabled")),
            ),
        )
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let Outcome::Finished { state, .. } = run(&graph, &config(), state, None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap()
    else {
        panic!("expected finished");
    };
    let Some(Entry::Turn(turn)) = state.conversation(&key("chat")).unwrap().entries().last() else {
        panic!("expected the agent's turn");
    };
    assert_eq!(
        results_of(turn),
        [
            (
                "write_file".to_owned(),
                true,
                "Tool write_file is not enabled for this run: this call was not executed."
                    .to_owned()
            ),
            ("echo".to_owned(), false, "echoed".to_owned()),
        ]
    );
    assert_eq!(writer.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[tokio::test]
async fn given_llm_invoked_with_awaiting_results_turn_when_run_then_fails_closed() {
    let state = state_with_call(tool_call_step("c1", "echo", json!({})));
    let client = mock(vec![text_step("fresh")]);
    let mut llm = llm_node(Arc::new(client.clone()), vec![Source::Config(key("base"))]);
    llm.tools = vec![Arc::new(EchoTool)];
    let graph = GraphBuilder::new(schema())
        .entry(nid("llm"))
        .node(nid("llm"), llm)
        .edge(
            nid("llm"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), state, None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        returned(&failure.error),
        crate::error::GraphError::Message(br_llm_messages::MessageError::TurnAwaitingResults)
    ));
    assert_eq!(client.calls(), 0, "the render refuses before any call");
    assert_eq!(
        failure
            .checkpoint
            .state
            .conversation(&key("chat"))
            .unwrap()
            .entries()
            .len(),
        2
    );
}

#[tokio::test]
async fn given_two_tools_set_same_key_when_run_then_node_fails_and_state_unchanged() {
    use crate::react::tool::{ToolFuture, ToolOutput};
    use crate::state::Config;
    use crate::update::Update;

    struct SetLog {
        spec: ToolSpec,
    }
    impl Tool for SetLog {
        fn spec(&self) -> &ToolSpec {
            &self.spec
        }
        fn call<'a>(
            &'a self,
            _arguments: serde_json::Value,
            _state: &'a State,
            _config: &'a Config,
        ) -> ToolFuture<'a> {
            Box::pin(async {
                Ok(ToolOutput::text(Vec::new()).with_updates(vec![Update::Set {
                    key: key("log"),
                    value: Value::list(vec![Value::str("x")]),
                }]))
            })
        }
    }

    let step = Step::new(
        vec![
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c1").unwrap(),
                name: ToolName::new("seta").unwrap(),
                arguments: json!({}),
            }),
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c2").unwrap(),
                name: ToolName::new("setb").unwrap(),
                arguments: json!({}),
            }),
        ],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap();
    let graph = GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(
            nid("tools"),
            tool_node(
                vec![
                    Arc::new(SetLog { spec: spec("seta") }),
                    Arc::new(SetLog { spec: spec("setb") }),
                ],
                None,
                None,
            ),
        )
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let failure = run(
        &graph,
        &config(),
        state_with_call(step),
        None,
        &ctx(),
        &mut channel,
    )
    .await
    .err()
    .unwrap();
    match failure.error {
        crate::error::GraphError::NodeFailed { source, .. } => {
            assert!(matches!(source, crate::error::NodeFault::Refused(_)));
        }
        other => panic!("expected NodeFailed, got {other}"),
    }
    assert!(
        failure
            .checkpoint
            .state
            .list(&key("log"))
            .unwrap()
            .is_empty()
    );
}
