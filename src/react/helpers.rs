use std::sync::Arc;

use br_llm_messages::{
    Author, Conversation, Entry, Perspective, Step, StreamEvent, ToolCall, Turn, TurnItem,
    TurnState, WireMessage, render,
};
use serde::de::DeserializeOwned;

use crate::error::GraphError;
use crate::graph::Context;
use crate::observe::Observer;
use crate::react::model::{Model, Request, StreamSink};
use crate::react::tool::Tool;
use crate::state::State;
use crate::value::Key;

struct ObserverSink<'a> {
    observer: &'a dyn Observer,
    key: &'a Key,
}

impl StreamSink for ObserverSink<'_> {
    fn event(&self, event: &StreamEvent) {
        self.observer.stream(self.key, event);
    }
}

pub fn wire(conversation: &Conversation, author: &Author) -> Result<Vec<WireMessage>, GraphError> {
    render(conversation, &Perspective::new(author.clone())).map_err(GraphError::from)
}

pub async fn complete(
    model: &dyn Model,
    request: Request,
    ctx: &Context,
    key: &Key,
) -> Result<Step, GraphError> {
    let sink = ObserverSink {
        observer: ctx.observer.as_ref(),
        key,
    };
    model
        .complete(request, &sink)
        .await
        .map_err(|error| GraphError::Model {
            message: error.to_string(),
        })
}

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

pub(crate) fn last_turn_state(conversation: &Conversation, author: &Author) -> Option<TurnState> {
    last_turn_by_author(conversation, author).map(Turn::state)
}

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

pub fn pending_unsafe_calls<'a>(
    state: &'a State,
    key: &Key,
    author: &Author,
    registry: &[Arc<dyn Tool>],
) -> Result<Vec<&'a ToolCall>, GraphError> {
    Ok(pending_calls(state, key, author)?
        .into_iter()
        .filter(|call| {
            registry
                .iter()
                .find(|tool| tool.spec().name == call.name)
                .map(|tool| !tool.safe())
                .unwrap_or(false)
        })
        .collect())
}
