use br_llm_messages::{
    Author, Conversation, Entry, Perspective, Step, Text, ToolCall, Turn, TurnItem, TurnState,
    WireMessage, render,
};
use serde::de::DeserializeOwned;

use crate::error::GraphError;
use crate::state::{Kind, Schema, State, Value};
use crate::value::Key;

/// `conversation` rendered for `author`; a refusal of the render gives
/// `Message`.
pub fn wire(conversation: &Conversation, author: &Author) -> Result<Vec<WireMessage>, GraphError> {
    render(conversation, &Perspective::new(author.clone())).map_err(GraphError::from)
}

/// The system text of a model call: none when the text is blank, which a
/// system prompt cannot hold.
pub(crate) fn system_text(text: String) -> Option<Text> {
    Text::new(text).ok()
}

/// Reads the first structured block of `step` into `T`. A step without one,
/// or a block that does not fit `T`, gives `Structured`: br-llm-clients-core
/// does not check a structured reply against its shape.
pub fn structured<T: DeserializeOwned>(step: &Step) -> Result<T, GraphError> {
    let value = step
        .structured()
        .next()
        .ok_or_else(|| GraphError::Structured {
            message: "the step carries no structured block".to_owned(),
        })?;
    serde_json::from_value(value.clone()).map_err(|error| GraphError::Structured {
        message: error.to_string(),
    })
}

/// The tool names an `enabled` key holds for this run.
pub(crate) fn enabled_names<'a>(state: &'a State, key: &Key) -> Result<Vec<&'a str>, GraphError> {
    Ok(state
        .list(key)?
        .iter()
        .filter_map(|value| match value {
            Value::Str(name) => Some(name.as_str()),
            Value::Int(_)
            | Value::Float(_)
            | Value::Bool(_)
            | Value::List(_)
            | Value::Conversation(_) => None,
        })
        .collect())
}

/// An `enabled` key must be a state key holding a list of strings.
pub(crate) fn check_enabled(schema: &Schema, enabled: Option<&Key>) -> Result<(), GraphError> {
    match enabled {
        Some(key) if schema.state.get(key) != Some(&Kind::list(Kind::Str)) => {
            Err(GraphError::EnabledKeyMismatch { key: key.clone() })
        }
        Some(_) | None => Ok(()),
    }
}

pub(crate) fn last_turn_by_author<'a>(
    conversation: &'a Conversation,
    author: &Author,
) -> Option<&'a Turn> {
    conversation
        .entries()
        .iter()
        .rev()
        .find_map(|entry| match entry {
            Entry::Turn(turn) if turn.author() == Some(author) => Some(turn),
            Entry::Turn(_) | Entry::UserInput(_) => None,
        })
}

/// The last step of a turn.
pub(crate) fn final_step(turn: &Turn) -> Option<&Step> {
    turn.items().iter().rev().find_map(|item| match item {
        TurnItem::Step(step) => Some(step),
        TurnItem::ToolResults(_) => None,
    })
}

/// The text of an answer: the JSON text of its structured block when it has
/// one, else its text blocks joined verbatim, with nothing between them.
pub(crate) fn answer_text(step: &Step) -> String {
    match step.structured().next() {
        Some(value) => value.to_string(),
        None => step.text().map(br_llm_messages::Text::as_str).collect(),
    }
}

pub(crate) fn last_turn_state(conversation: &Conversation, author: &Author) -> Option<TurnState> {
    last_turn_by_author(conversation, author).map(Turn::state)
}

/// The pending calls of `author`'s last turn in the conversation `key`, in
/// call order: none when that turn does not await tool results, or when
/// `author` has no turn.
pub fn pending_calls<'a>(
    state: &'a State,
    key: &Key,
    author: &Author,
) -> Result<Vec<&'a ToolCall>, GraphError> {
    let conversation = state.conversation(key)?;
    let Some(turn) = last_turn_by_author(conversation, author) else {
        return Ok(Vec::new());
    };
    let TurnState::AwaitingToolResults { pending } = turn.state() else {
        return Ok(Vec::new());
    };
    let mut calls = Vec::new();
    for item in turn.items() {
        if let TurnItem::Step(step) = item {
            for call in step.tool_calls() {
                if pending.contains(&call.id) {
                    calls.push(call);
                }
            }
        }
    }
    Ok(calls)
}
