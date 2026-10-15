use br_llm_messages::{
    AssistantBlock, Author, Conversation, InputId, Step, StopReason, Text, ToolCall, ToolCallId,
    ToolName, ToolResult, Turn, TurnId, UserBlock, UserInput, UserSource,
};

use crate::error::GraphError;
use crate::state::{Schema, State, StateKey, Value};
use crate::update::Update;
use crate::value::Key;

struct Keys {
    count: StateKey<i64>,
    ratio: StateKey<f64>,
    ratios: StateKey<Vec<f64>>,
    notes: StateKey<Vec<String>>,
    chat: StateKey<Conversation>,
}

fn declared() -> (Keys, State) {
    let mut schema = Schema::builder();
    let keys = Keys {
        count: schema.state("count").unwrap(),
        ratio: schema.state("ratio").unwrap(),
        ratios: schema.state("ratios").unwrap(),
        notes: schema.state("notes").unwrap(),
        chat: schema.state("chat").unwrap(),
    };
    let schema = schema.build();
    let values = schema
        .state
        .keys()
        .map(|key| (key.clone(), schema.default_value(key).unwrap()))
        .collect();
    (keys, State::new(schema, values).unwrap())
}

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

#[test]
fn given_a_set_through_a_typed_key_when_applied_then_read_back_as_its_type() {
    let (keys, mut state) = declared();
    let set = keys.count.set(4).unwrap();
    assert_eq!(
        set,
        Update::Set {
            key: key("count"),
            value: Value::int(4)
        }
    );
    state
        .apply_batch(&[
            set,
            keys.notes
                .set(vec!["a".to_owned(), "b".to_owned()])
                .unwrap(),
        ])
        .unwrap();
    assert_eq!(state.get(&keys.count).unwrap(), 4);
    assert_eq!(state.get(&keys.notes).unwrap(), vec!["a", "b"]);
}

#[test]
fn given_an_append_through_a_typed_list_key_when_applied_then_at_the_end() {
    let (keys, mut state) = declared();
    let append = keys.notes.append("first").unwrap();
    assert_eq!(
        append,
        Update::Append {
            key: key("notes"),
            value: Value::str("first")
        }
    );
    state
        .apply_batch(&[append, keys.notes.append("second").unwrap()])
        .unwrap();
    assert_eq!(state.get(&keys.notes).unwrap(), vec!["first", "second"]);
}

#[test]
fn given_a_float_that_is_not_finite_when_set_or_appended_then_float_not_finite() {
    let (keys, _) = declared();
    assert!(matches!(
        keys.ratio.set(f64::NAN),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        keys.ratios.set(vec![0.5, f64::NEG_INFINITY]),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        keys.ratios.append(f64::INFINITY),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        keys.ratio.given(f64::NAN),
        Err(GraphError::FloatNotFinite)
    ));
    assert_eq!(keys.ratio.set(0.25).unwrap().key(), &key("ratio"));
}

#[test]
fn given_a_typed_key_when_given_then_the_input_by_name() {
    let (keys, _) = declared();
    assert_eq!(
        keys.notes.given(vec!["x".to_owned()]).unwrap(),
        (key("notes"), Value::list(vec![Value::str("x")]))
    );
}

#[test]
fn given_conversation_updates_through_a_typed_key_when_applied_then_the_conversation_grows() {
    let (keys, mut state) = declared();
    let author = Author::new("agent", "agent").unwrap();
    let input = UserInput::new(
        InputId::new("i-1").unwrap(),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new("look it up").unwrap())],
    )
    .unwrap();
    let call = Step::new(
        vec![AssistantBlock::ToolCall(ToolCall {
            id: ToolCallId::new("c1").unwrap(),
            name: ToolName::new("search").unwrap(),
            arguments: serde_json::json!({}),
        })],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap();
    let turn = TurnId::new("t1").unwrap();
    let result = ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("search").unwrap(),
        vec![],
        false,
    );
    let answer = Step::new(
        vec![AssistantBlock::Text {
            text: Text::new("found").unwrap(),
        }],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    let updates = [
        keys.chat.input(input.clone()),
        keys.chat
            .push_turn(Turn::new(turn.clone(), Some(author), call)),
        keys.chat.push_result(turn.clone(), result),
        keys.chat.push_step(turn.clone(), answer),
    ];
    assert_eq!(
        updates[0],
        Update::Input {
            key: key("chat"),
            input
        }
    );
    state.apply_batch(&updates).unwrap();
    let chat = state.get(&keys.chat).unwrap();
    assert_eq!(chat.entries().len(), 2);
    assert_eq!(chat.turn(&turn).unwrap().items().len(), 3);
}

#[test]
fn given_a_typed_set_on_a_state_declaring_the_name_with_another_kind_when_applied_then_kind_mismatch()
 {
    let mut other = Schema::builder();
    let count: StateKey<String> = other.state("count").unwrap();
    let (_, mut state) = declared();
    let before = state.clone();
    assert!(matches!(
        state.apply_batch(&[count.set("three").unwrap()]),
        Err(GraphError::KindMismatch { .. })
    ));
    assert_eq!(state, before);
}
