//! The conversation updates: `Input`, `PushTurn`, `PushStep` and
//! `PushResult` on a conversation key, refused on another kind, a refusal of
//! the conversation itself reported as `Message`.

use br_llm_messages::AssistantBlock;
use br_llm_messages::{
    Author, Entry, InputId, MessageError, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    ToolResult, Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource,
};

use super::tests::{key, state};
use crate::error::GraphError;
use crate::state::value::Value;
use crate::update::Update;

pub(super) fn input(body: &str) -> UserInput {
    UserInput::new(
        InputId::new(format!("i-{body}")).unwrap(),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

pub(super) fn turn(id: &str) -> Turn {
    let step = Step::new(
        vec![AssistantBlock::Text {
            text: Text::new("hello").unwrap(),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    Turn::new(
        TurnId::new(id).unwrap(),
        Some(Author::new("agent", "agent").unwrap()),
        step,
    )
}

#[test]
fn given_input_on_conversation_when_applied_then_entry_added() {
    let mut state = state();
    state
        .apply_batch(&[Update::Input {
            key: key("chat"),
            input: input("hi"),
        }])
        .unwrap();
    assert_eq!(state.conversation(&key("chat")).unwrap().entries().len(), 1);
}

#[test]
fn given_input_on_non_conversation_when_applied_then_refused() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[Update::Input {
            key: key("count"),
            input: input("hi"),
        }]),
        Err(GraphError::NotConversation { .. })
    ));
}

#[test]
fn given_input_whose_id_the_conversation_holds_when_applied_then_message_error_and_unchanged() {
    let mut state = state();
    state
        .apply_batch(&[Update::Input {
            key: key("chat"),
            input: input("hi"),
        }])
        .unwrap();
    let before = state.clone();
    let result = state.apply_batch(&[
        Update::Append {
            key: key("items"),
            value: Value::str("kept only on success"),
        },
        Update::Input {
            key: key("chat"),
            input: input("hi"),
        },
    ]);
    assert!(matches!(
        result,
        Err(GraphError::Message(MessageError::DuplicateEntryId { ref id })) if id == "i-hi"
    ));
    assert_eq!(state, before);
}

#[test]
fn given_turn_whose_id_is_an_input_id_when_applied_then_message_error() {
    let mut state = state();
    state
        .apply_batch(&[Update::Input {
            key: key("chat"),
            input: input("hi"),
        }])
        .unwrap();
    assert!(matches!(
        state.apply_batch(&[Update::PushTurn {
            key: key("chat"),
            turn: turn("i-hi"),
        }]),
        Err(GraphError::Message(MessageError::DuplicateEntryId { .. }))
    ));
}

#[test]
fn given_push_turn_when_conversation_then_added() {
    let mut state = state();
    state
        .apply_batch(&[Update::PushTurn {
            key: key("chat"),
            turn: turn("t1"),
        }])
        .unwrap();
    assert_eq!(state.conversation(&key("chat")).unwrap().entries().len(), 1);
}

#[test]
fn given_push_turn_on_non_conversation_when_applied_then_refused() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[Update::PushTurn {
            key: key("count"),
            turn: turn("t1"),
        }]),
        Err(GraphError::NotConversation { .. })
    ));
}

#[test]
fn given_push_result_on_matching_turn_when_applied_then_added() {
    let mut state = state();
    let call = ToolCall {
        id: ToolCallId::new("c1").unwrap(),
        name: ToolName::new("echo").unwrap(),
        arguments: serde_json::json!({}),
    };
    let step = Step::new(
        vec![AssistantBlock::ToolCall(call)],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap();
    let turn = Turn::new(
        TurnId::new("t1").unwrap(),
        Some(Author::new("agent", "agent").unwrap()),
        step,
    );
    state
        .apply_batch(&[Update::PushTurn {
            key: key("chat"),
            turn,
        }])
        .unwrap();
    let result = ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        Vec::new(),
        false,
    );
    state
        .apply_batch(&[Update::PushResult {
            key: key("chat"),
            turn: TurnId::new("t1").unwrap(),
            result,
        }])
        .unwrap();
    let convo = state.conversation(&key("chat")).unwrap();
    let has_results = convo.entries().iter().any(|entry| match entry {
        Entry::Turn(turn) => turn
            .items()
            .iter()
            .any(|item| matches!(item, TurnItem::ToolResults(_))),
        Entry::UserInput(_) => false,
    });
    assert!(has_results);
}

#[test]
fn given_push_result_on_non_conversation_when_applied_then_refused() {
    let mut state = state();
    let result = ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        Vec::new(),
        false,
    );
    assert!(matches!(
        state.apply_batch(&[Update::PushResult {
            key: key("count"),
            turn: TurnId::new("t1").unwrap(),
            result,
        }]),
        Err(GraphError::NotConversation { .. })
    ));
}

#[test]
fn given_push_step_to_awaiting_turn_when_applied_then_appended() {
    let mut state = state();
    let call = ToolCall {
        id: ToolCallId::new("c1").unwrap(),
        name: ToolName::new("echo").unwrap(),
        arguments: serde_json::json!({}),
    };
    let step = Step::new(
        vec![AssistantBlock::ToolCall(call)],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap();
    let turn = Turn::new(
        TurnId::new("t1").unwrap(),
        Some(Author::new("agent", "agent").unwrap()),
        step,
    );
    state
        .apply_batch(&[Update::PushTurn {
            key: key("chat"),
            turn,
        }])
        .unwrap();
    let result = ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        Vec::new(),
        false,
    );
    state
        .apply_batch(&[Update::PushResult {
            key: key("chat"),
            turn: TurnId::new("t1").unwrap(),
            result,
        }])
        .unwrap();
    let next = Step::new(
        vec![AssistantBlock::Text {
            text: Text::new("done").unwrap(),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    state
        .apply_batch(&[Update::PushStep {
            key: key("chat"),
            turn: TurnId::new("t1").unwrap(),
            step: next,
        }])
        .unwrap();
    let convo = state.conversation(&key("chat")).unwrap();
    let turn = convo.turn(&TurnId::new("t1").unwrap()).unwrap();
    assert_eq!(turn.items().len(), 3);
}

#[test]
fn given_push_step_to_missing_turn_when_applied_then_message_error() {
    let mut state = state();
    let step = Step::new(
        vec![AssistantBlock::Text {
            text: Text::new("x").unwrap(),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    assert!(matches!(
        state.apply_batch(&[Update::PushStep {
            key: key("chat"),
            turn: TurnId::new("nope").unwrap(),
            step,
        }]),
        Err(GraphError::Message(_))
    ));
}
