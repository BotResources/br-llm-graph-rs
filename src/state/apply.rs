use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::state::State;
use crate::state::value::Value;
use crate::update::Update;
use crate::value::Key;

impl State {
    pub fn apply_batch(&mut self, updates: &[Update]) -> Result<(), GraphError> {
        let mut seen_set: Vec<&Key> = Vec::new();
        for update in updates {
            if let Update::Set { key, .. } = update {
                if seen_set.contains(&key) {
                    return Err(GraphError::SetConflict { key: key.clone() });
                }
                seen_set.push(key);
            }
        }
        let mut working = self.clone();
        for update in updates {
            working.apply_one(update)?;
        }
        *self = working;
        Ok(())
    }

    fn apply_one(&mut self, update: &Update) -> Result<(), GraphError> {
        match update {
            Update::Set { key, value } => self.apply_set(key, value),
            Update::Append { key, value } => self.apply_append(key, value),
            Update::Input { key, input } => {
                self.conversation_mut(key)?.push_input(input.clone());
                Ok(())
            }
            Update::PushTurn { key, turn } => {
                self.conversation_mut(key)?.push_turn(turn.clone())?;
                Ok(())
            }
            Update::PushStep { key, turn, step } => {
                self.conversation_mut(key)?.push_step(turn, step.clone())?;
                Ok(())
            }
            Update::PushResult { key, turn, result } => {
                self.conversation_mut(key)?
                    .push_result(turn, result.clone())?;
                Ok(())
            }
        }
    }

    fn apply_set(&mut self, key: &Key, value: &Value) -> Result<(), GraphError> {
        let kind = self
            .kind_of(key)
            .ok_or_else(|| GraphError::UnknownKey { key: key.clone() })?;
        if !value.matches(kind) {
            return Err(GraphError::KindMismatch {
                key: key.clone(),
                expected: kind.clone(),
                found: value.tag(),
            });
        }
        self.values_mut().insert(key.clone(), value.clone());
        Ok(())
    }

    fn apply_append(&mut self, key: &Key, value: &Value) -> Result<(), GraphError> {
        let element = match self.kind_of(key) {
            Some(Kind::List { element }) => element.as_ref().clone(),
            Some(_) | None => return Err(GraphError::AppendNotList { key: key.clone() }),
        };
        if !value.matches(&element) {
            return Err(GraphError::KindMismatch {
                key: key.clone(),
                expected: element,
                found: value.tag(),
            });
        }
        match self.values_mut().get_mut(key) {
            Some(Value::List(items)) => {
                items.push(value.clone());
                Ok(())
            }
            Some(
                Value::Int(_)
                | Value::Float(_)
                | Value::Str(_)
                | Value::Bool(_)
                | Value::Conversation(_),
            )
            | None => Err(GraphError::AppendNotList { key: key.clone() }),
        }
    }

    fn conversation_mut(&mut self, key: &Key) -> Result<&mut Conversation, GraphError> {
        match self.values_mut().get_mut(key) {
            Some(Value::Conversation(conversation)) => Ok(conversation),
            Some(
                Value::Int(_) | Value::Float(_) | Value::Str(_) | Value::Bool(_) | Value::List(_),
            )
            | None => Err(GraphError::NotConversation { key: key.clone() }),
        }
    }
}

#[cfg(test)]
#[path = "apply_tests.rs"]
mod tests;
