//! Replies that cannot be used, what the LLM call, the ReAct agent and the
//! generator share: which replies they are, why, the reminder that asks for
//! one again, and the error of a node left with no retry.

use br_llm_clients_core::{ClientError, OutputMode};
use br_llm_messages::{Step, Text, WireMessage, WireUserBlock};

use crate::error::GraphError;
use crate::graph::NodeError;
use crate::react::fit::fit;
use crate::react::given::{ModelNode, Prompt};
use crate::state::{Config, State};

/// The reminder sent with a reply asked for again, by default; `{reason}`
/// is replaced by why the reply could not be used.
pub(crate) const REMINDER: &str = "Your reply could not be used: {reason}. Reply again.";

/// Where the reason goes in a reminder.
const REASON: &str = "{reason}";

/// Why a structured reply that calls no tool and holds no structured block
/// cannot be used: the words of an unreadable verdict, and of `structured`.
const NO_BLOCK: &str = "the step carries no structured block";

/// Why a reply cannot be used.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Unusable {
    /// The client refused it, as unusable (`ReplyUnusable`) or malformed
    /// (`MalformedResponse`).
    Refused(ClientError),
    /// With a structured output mode, a reply that calls no tool and whose
    /// structured block is missing or does not fit the shape: why.
    Unfit(String),
}

impl Unusable {
    /// Why the reply could not be used, as a reminder reads it: the client's
    /// error, or why the reply does not fit.
    pub(crate) fn reason(&self) -> String {
        match self {
            Unusable::Refused(error) => error.to_string(),
            Unusable::Unfit(reason) => reason.clone(),
        }
    }

    /// The error of a node left with no retry: `Model`, carrying the error
    /// of the client, or `Structured`, carrying the reason.
    pub(crate) fn into_error(self) -> GraphError {
        match self {
            Unusable::Refused(error) => GraphError::Model(error),
            Unusable::Unfit(message) => GraphError::Structured { message },
        }
    }
}

/// Whether the client's `error` refuses the reply as unusable or malformed:
/// a reply that may be asked for again, unlike every other error of a
/// client.
pub(crate) fn refuses(error: &ClientError) -> bool {
    matches!(
        error,
        ClientError::ReplyUnusable { .. } | ClientError::MalformedResponse { .. }
    )
}

/// What a model call gave, judged in `mode`: the step when it can be used,
/// why it cannot otherwise; any other error as it is.
pub(crate) fn judged(
    result: Result<Step, GraphError>,
    mode: &OutputMode,
) -> Result<Result<Step, Unusable>, GraphError> {
    let step = match result {
        Ok(step) => step,
        Err(GraphError::Model(error)) if refuses(&error) => {
            return Ok(Err(Unusable::Refused(error)));
        }
        Err(error) => return Err(error),
    };
    Ok(match unfit(&step, mode) {
        Some(reason) => Err(Unusable::Unfit(reason)),
        None => Ok(step),
    })
}

/// Why `step` cannot be used in `mode`, if it cannot: with a structured
/// mode, a step that calls no tool and whose first structured block is
/// missing or does not fit the shape. A step that calls tools is not
/// checked, nor is any step in the plain mode.
pub(crate) fn unfit(step: &Step, mode: &OutputMode) -> Option<String> {
    let OutputMode::Structured(shape) = mode else {
        return None;
    };
    if step.tool_calls().next().is_some() {
        return None;
    }
    match step.structured().next() {
        None => Some(NO_BLOCK.to_owned()),
        Some(value) => fit(shape, value).err().map(|misfit| misfit.to_string()),
    }
}

/// The reminder of a reply asked for again because of `reason`: the text
/// `given` resolves to for the context `facts`, or `REMINDER` when it is not
/// given, each `{reason}` in it replaced by `reason`. `None` when it
/// resolves to a blank text: the request is then sent alone, as a blank
/// system sends no system text.
pub(crate) fn reminder<'a, N: ModelNode>(
    given: Option<&'a Prompt<N>>,
    state: &'a State,
    config: &'a Config,
    facts: N::Facts<'a>,
    reason: &str,
) -> Result<Option<Text>, NodeError> {
    let template = match given {
        Some(prompt) => prompt.give(state, config, facts)?,
        None => REMINDER.to_owned(),
    };
    Ok(Text::new(template.replace(REASON, reason)).ok())
}

/// `messages` followed by the reminder `text`, when there is one: one more
/// user message; or, when they end with a relay (the entries an agent has
/// not answered yet, which only the last message may be), one more text of
/// that relay.
pub(crate) fn reminded(mut messages: Vec<WireMessage>, text: Option<Text>) -> Vec<WireMessage> {
    let Some(text) = text else {
        return messages;
    };
    match messages.last_mut() {
        Some(WireMessage::Relay { content }) => content.push(text),
        Some(WireMessage::User { .. } | WireMessage::Assistant { .. }) | None => {
            messages.push(WireMessage::User {
                content: vec![WireUserBlock::Text { text }],
            });
        }
    }
    messages
}

#[cfg(test)]
#[path = "replies_tests.rs"]
mod tests;
