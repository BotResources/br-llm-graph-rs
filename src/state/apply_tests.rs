use std::collections::BTreeMap;

use br_llm_messages::AssistantBlock;
use br_llm_messages::{
    Author, Conversation, Entry, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    ToolResult, Turn, TurnId, TurnItem, UserBlock, UserInput, UserSource,
};

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::schema::Schema;
use crate::state::state::State;
use crate::state::value::Value;
use crate::update::Update;
use crate::value::Key;

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

fn state() -> State {
    let schema = Schema::builder()
        .state(key("count"), Kind::Int)
        .state(key("items"), Kind::list(Kind::Str))
        .state(key("chat"), Kind::Conversation)
        .build();
    let mut values = BTreeMap::new();
    values.insert(key("count"), Value::int(0));
    values.insert(key("items"), Value::list(Vec::new()));
    values.insert(key("chat"), Value::conversation(Conversation::new()));
    State::new(schema, values).unwrap()
}

fn input(body: &str) -> UserInput {
    UserInput::new(
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

fn turn(id: &str) -> Turn {
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
        Some(Author::new("agent").unwrap()),
        step,
    )
}

#[test]
fn given_set_when_kind_matches_then_applied() {
    let mut state = state();
    state
        .apply_batch(&[Update::Set {
            key: key("count"),
            value: Value::int(5),
        }])
        .unwrap();
    assert_eq!(state.int(&key("count")).unwrap(), 5);
}

#[test]
fn given_set_when_kind_wrong_then_refused_and_unchanged() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[Update::Set {
            key: key("count"),
            value: Value::str("no"),
        }]),
        Err(GraphError::KindMismatch { .. })
    ));
    assert_eq!(state.int(&key("count")).unwrap(), 0);
}

#[test]
fn given_append_when_list_then_pushed() {
    let mut state = state();
    state
        .apply_batch(&[Update::Append {
            key: key("items"),
            value: Value::str("a"),
        }])
        .unwrap();
    assert_eq!(state.list(&key("items")).unwrap().len(), 1);
}

#[test]
fn given_append_on_non_list_when_applied_then_refused() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[Update::Append {
            key: key("count"),
            value: Value::int(1),
        }]),
        Err(GraphError::AppendNotList { .. })
    ));
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
        Some(Author::new("agent").unwrap()),
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

#[test]
fn given_two_sets_on_same_key_when_batched_then_conflict() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[
            Update::Set {
                key: key("count"),
                value: Value::int(1),
            },
            Update::Set {
                key: key("count"),
                value: Value::int(2),
            },
        ]),
        Err(GraphError::SetConflict { .. })
    ));
}

#[test]
fn given_two_appends_on_same_key_when_batched_then_both_applied() {
    let mut state = state();
    state
        .apply_batch(&[
            Update::Append {
                key: key("items"),
                value: Value::str("a"),
            },
            Update::Append {
                key: key("items"),
                value: Value::str("b"),
            },
        ])
        .unwrap();
    assert_eq!(state.list(&key("items")).unwrap().len(), 2);
}

#[test]
fn given_batch_with_a_late_failure_when_applied_then_all_or_nothing() {
    let mut state = state();
    let outcome = state.apply_batch(&[
        Update::Set {
            key: key("count"),
            value: Value::int(7),
        },
        Update::Append {
            key: key("count"),
            value: Value::int(1),
        },
    ]);
    assert!(outcome.is_err());
    assert_eq!(state.int(&key("count")).unwrap(), 0);
    assert!(state.list(&key("items")).unwrap().is_empty());
}
