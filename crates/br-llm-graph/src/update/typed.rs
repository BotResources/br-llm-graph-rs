//! Updates through a typed key: each takes only a value of the key's type.

use br_llm_messages::{Conversation, Step, ToolResult, Turn, TurnId, UserInput};

use crate::error::GraphError;
use crate::state::{StateKey, Value, ValueType};
use crate::update::Update;
use crate::value::Key;

impl<T: ValueType> StateKey<T> {
    /// A `Set` of this key to `value`. A float that is not finite is refused
    /// (`FloatNotFinite`), in a list too.
    ///
    /// ```
    /// use br_llm_graph::{Schema, StateKey, Update, Value};
    ///
    /// let mut schema = Schema::builder();
    /// let attempts: StateKey<i64> = schema.state("attempts").unwrap();
    /// assert_eq!(
    ///     attempts.set(3).unwrap(),
    ///     Update::Set { key: attempts.key().clone(), value: Value::int(3) }
    /// );
    /// ```
    pub fn set(&self, value: impl Into<T>) -> Result<Update, GraphError> {
        Ok(Update::Set {
            key: self.key().clone(),
            value: value.into().into_value()?,
        })
    }

    /// The input `value` given to this key when a graph starts, for
    /// `Graph::start_state`. A float that is not finite is refused
    /// (`FloatNotFinite`), in a list too.
    pub fn given(&self, value: impl Into<T>) -> Result<(Key, Value), GraphError> {
        Ok((self.key().clone(), value.into().into_value()?))
    }
}

impl<T: ValueType> StateKey<Vec<T>> {
    /// An `Append` of `item` at the end of this list key. A float that is not
    /// finite is refused (`FloatNotFinite`).
    pub fn append(&self, item: impl Into<T>) -> Result<Update, GraphError> {
        Ok(Update::Append {
            key: self.key().clone(),
            value: item.into().into_value()?,
        })
    }
}

impl StateKey<Conversation> {
    /// An `Input`: `input` added at the end of this conversation.
    pub fn input(&self, input: UserInput) -> Update {
        Update::Input {
            key: self.key().clone(),
            input,
        }
    }

    /// A `PushTurn`: `turn` added at the end of this conversation.
    pub fn push_turn(&self, turn: Turn) -> Update {
        Update::PushTurn {
            key: self.key().clone(),
            turn,
        }
    }

    /// A `PushStep`: `step` added to the turn `turn` of this conversation.
    pub fn push_step(&self, turn: TurnId, step: Step) -> Update {
        Update::PushStep {
            key: self.key().clone(),
            turn,
            step,
        }
    }

    /// A `PushResult`: `result` added to the turn `turn` of this conversation.
    pub fn push_result(&self, turn: TurnId, result: ToolResult) -> Update {
        Update::PushResult {
            key: self.key().clone(),
            turn,
            result,
        }
    }
}

#[cfg(test)]
#[path = "typed_tests.rs"]
mod tests;
