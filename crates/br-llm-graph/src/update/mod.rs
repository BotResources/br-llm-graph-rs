//! Updates: the changes a node returns, or the control channel brings, that the
//! run alone applies to the state.
//!
//! A runner builds them through typed keys (`StateKey::set`, `append`,
//! `input`...), which take only values of the key's kind; `Update` itself is
//! the by-name form, for what is known only at run time, and its JSON shape
//! (tagged by `type`, snake case).

mod typed;

use br_llm_messages::{Step, ToolResult, Turn, TurnId, UserInput};

use crate::state::Value;
use crate::value::Key;

/// One change to the state. A batch of updates is applied in the order given,
/// all or nothing; two `Set` of one key in one batch are refused before
/// anything is applied (`SetConflict`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Update {
    /// Replaces the value of a declared key with a value of its kind
    /// (`UnknownKey`, `KindMismatch`); on a conversation, the whole
    /// conversation.
    Set {
        /// The state key.
        key: Key,
        /// The value.
        value: Value,
    },
    /// Adds a value at the end of a list key (`UnknownKey`, `AppendNotList`,
    /// `KindMismatch` naming the element kind).
    Append {
        /// The state key.
        key: Key,
        /// The element appended.
        value: Value,
    },
    /// Adds a user input at the end of a conversation key (`UnknownKey`,
    /// `NotConversation`; a refusal of the conversation is `Message`).
    Input {
        /// The conversation key.
        key: Key,
        /// The user input.
        input: UserInput,
    },
    /// Adds a turn at the end of a conversation key, under the same refusals
    /// as `Input`.
    PushTurn {
        /// The conversation key.
        key: Key,
        /// The turn.
        turn: Turn,
    },
    /// Adds a step to a turn of a conversation key, under the same refusals as
    /// `Input`.
    PushStep {
        /// The conversation key.
        key: Key,
        /// The turn the step goes to.
        turn: TurnId,
        /// The step.
        step: Step,
    },
    /// Adds a tool result to a turn of a conversation key, under the same
    /// refusals as `Input`.
    PushResult {
        /// The conversation key.
        key: Key,
        /// The turn the result goes to.
        turn: TurnId,
        /// The tool result.
        result: ToolResult,
    },
}

impl Update {
    /// The key the update changes.
    pub fn key(&self) -> &Key {
        match self {
            Update::Set { key, .. }
            | Update::Append { key, .. }
            | Update::Input { key, .. }
            | Update::PushTurn { key, .. }
            | Update::PushStep { key, .. }
            | Update::PushResult { key, .. } => key,
        }
    }
}

impl std::fmt::Display for Update {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Update::Set { key, value } => write!(f, "set {key} = {value}"),
            Update::Append { key, value } => write!(f, "append {value} to {key}"),
            Update::Input { key, .. } => write!(f, "input into {key}"),
            Update::PushTurn { key, turn } => write!(f, "push turn {} into {key}", turn.id()),
            Update::PushStep { key, turn, .. } => write!(f, "push step into turn {turn} of {key}"),
            Update::PushResult { key, turn, .. } => {
                write!(f, "push result into turn {turn} of {key}")
            }
        }
    }
}

#[cfg(test)]
#[path = "update_tests.rs"]
mod tests;
