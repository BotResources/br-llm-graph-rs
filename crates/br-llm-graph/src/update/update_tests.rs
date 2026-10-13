use br_llm_messages::{
    AssistantBlock, Author, Step, StopReason, Text, ToolCallId, ToolName, ToolResult, Turn, TurnId,
    UserBlock, UserInput, UserSource,
};

use super::*;

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

fn round_trip(update: Update) {
    let json = serde_json::to_string(&update).unwrap();
    assert_eq!(serde_json::from_str::<Update>(&json).unwrap(), update);
}

#[test]
fn given_set_when_round_tripped_then_identical() {
    round_trip(Update::Set {
        key: key("count"),
        value: Value::int(3),
    });
}

#[test]
fn given_append_when_round_tripped_then_identical() {
    round_trip(Update::Append {
        key: key("items"),
        value: Value::str("x"),
    });
}

#[test]
fn given_input_when_round_tripped_then_identical() {
    let input = UserInput::new(
        UserSource::runtime("wake").unwrap(),
        Some(Author::new("scheduler").unwrap()),
        vec![UserBlock::text(Text::new("done").unwrap())],
    )
    .unwrap();
    round_trip(Update::Input {
        key: key("chat"),
        input,
    });
}

#[test]
fn given_push_turn_when_round_tripped_then_identical() {
    let step = Step::new(
        vec![AssistantBlock::Text {
            text: Text::new("hi").unwrap(),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    round_trip(Update::PushTurn {
        key: key("chat"),
        turn: Turn::new(
            TurnId::new("t1").unwrap(),
            Some(Author::new("a").unwrap()),
            step,
        ),
    });
}

#[test]
fn given_push_step_when_round_tripped_then_identical() {
    let step = Step::new(
        vec![AssistantBlock::Text {
            text: Text::new("more").unwrap(),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    round_trip(Update::PushStep {
        key: key("chat"),
        turn: TurnId::new("t1").unwrap(),
        step,
    });
}

#[test]
fn given_push_result_when_round_tripped_then_identical() {
    round_trip(Update::PushResult {
        key: key("chat"),
        turn: TurnId::new("t1").unwrap(),
        result: ToolResult::new(
            ToolCallId::new("c1").unwrap(),
            ToolName::new("echo").unwrap(),
            Vec::new(),
            false,
        ),
    });
}

#[test]
fn given_set_when_serialized_then_type_tagged() {
    let json = serde_json::to_value(Update::Set {
        key: key("count"),
        value: Value::int(1),
    })
    .unwrap();
    assert_eq!(json["type"], serde_json::json!("set"));
}
