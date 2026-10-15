//! The public helpers of the react module: reading a structured block, and
//! the pending calls of an author's last turn.

use br_llm_messages::{AssistantBlock, Step, StopReason, ToolCall, ToolCallId, ToolName, TurnId};
use serde_json::json;

use crate::react::helpers::{pending_calls, structured};
use crate::react::react_node_support::*;
use crate::react::test_support::*;
use crate::state::State;

#[test]
fn given_structured_step_when_read_then_deserializes() {
    #[derive(serde::Deserialize)]
    struct Verdict {
        achieved: bool,
    }
    let step = structured_step(json!({ "achieved": true }));
    let verdict: Verdict = structured(&step).unwrap();
    assert!(verdict.achieved);
}

#[test]
fn given_text_only_step_when_structured_then_error() {
    #[derive(serde::Deserialize)]
    struct Verdict {
        #[allow(dead_code)]
        achieved: bool,
    }
    let step = text_step("no json here");
    assert!(structured::<Verdict>(&step).is_err());
}

#[test]
fn given_a_turn_awaiting_results_when_pending_calls_read_then_the_unanswered_calls_in_call_order() {
    let step = Step::new(
        vec![
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c1").unwrap(),
                name: ToolName::new("write_file").unwrap(),
                arguments: json!({}),
            }),
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c2").unwrap(),
                name: ToolName::new("echo").unwrap(),
                arguments: json!({}),
            }),
        ],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap();
    let state = state_with_call(step);
    let names = |state: &State| -> Vec<String> {
        pending_calls(state, &key("chat"), &author())
            .unwrap()
            .iter()
            .map(|call| call.name.as_str().to_owned())
            .collect()
    };
    assert_eq!(names(&state), ["write_file", "echo"]);

    let mut answered = state.clone();
    answered
        .apply_batch(&[crate::update::Update::PushResult {
            key: key("chat"),
            turn: TurnId::new("t1").unwrap(),
            result: br_llm_messages::ToolResult::new(
                ToolCallId::new("c1").unwrap(),
                ToolName::new("write_file").unwrap(),
                Vec::new(),
                false,
            ),
        }])
        .unwrap();
    assert_eq!(names(&answered), ["echo"]);
    assert!(
        pending_calls(&seeded_state(), &key("chat"), &author())
            .unwrap()
            .is_empty(),
        "an author without a turn has no pending call"
    );
}
