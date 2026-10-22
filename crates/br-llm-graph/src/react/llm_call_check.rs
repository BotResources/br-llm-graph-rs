//! The build check of an `LlmCall` on its conversation key, refused with the
//! error a read of it would give. Its output is checked as any text output
//! (`TextOutput`).

use crate::error::GraphError;
use crate::state::{Kind, Schema};
use crate::value::Key;

/// A conversation key must be a conversation state key: the error a read of
/// it would give otherwise.
pub(crate) fn check_conversation(schema: &Schema, key: &Key) -> Result<(), GraphError> {
    match schema.state.get(key) {
        Some(Kind::Conversation) => Ok(()),
        Some(kind) => Err(GraphError::KindMismatch {
            key: key.clone(),
            expected: Kind::Conversation,
            found: kind.tag(),
        }),
        None => Err(GraphError::UnknownKey { key: key.clone() }),
    }
}
