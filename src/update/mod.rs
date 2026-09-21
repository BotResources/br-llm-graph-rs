use br_llm_messages::{Step, ToolResult, Turn, TurnId, UserInput};

use crate::state::Value;
use crate::value::Key;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Update {
    Set {
        key: Key,
        value: Value,
    },
    Append {
        key: Key,
        value: Value,
    },
    Input {
        key: Key,
        input: UserInput,
    },
    PushTurn {
        key: Key,
        turn: Turn,
    },
    PushStep {
        key: Key,
        turn: TurnId,
        step: Step,
    },
    PushResult {
        key: Key,
        turn: TurnId,
        result: ToolResult,
    },
}

impl Update {
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
            Update::PushTurn { key, turn } => write!(f, "push turn {turn} into {key}"),
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
