//! The build checks of an `LlmCall` on the keys it names itself: its
//! conversation key and its output key, each refused with the error its use
//! would give. A `RunnerCall` checks its outputs the same way.

use crate::error::GraphError;
use crate::graph::{CheckSite, Output};
use crate::react::llm_call::LlmOutput;
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

/// The output must take a string: the error applying the answer would give
/// otherwise. In a map body, a `Set` is refused (`MapBodySet`).
pub(crate) fn check_output(
    schema: &Schema,
    output: &LlmOutput,
    site: CheckSite,
) -> Result<(), GraphError> {
    check_text_output(schema, &output.written(), site)
}

/// A string written through `output` (the answer of an `LlmCall`, the answer
/// or the thread of a `RunnerCall`) must fit: the error applying it would
/// give otherwise. In a map body, a `Set` is refused (`MapBodySet`).
pub(crate) fn check_text_output(
    schema: &Schema,
    output: &Output,
    site: CheckSite,
) -> Result<(), GraphError> {
    let (key, expected) = match output {
        Output::Set(key) if site == CheckSite::MapBody => {
            return Err(GraphError::MapBodySet { key: key.clone() });
        }
        Output::Set(key) => match schema.state.get(key) {
            Some(Kind::Str) => return Ok(()),
            Some(kind) => (key, kind),
            None => return Err(GraphError::UnknownKey { key: key.clone() }),
        },
        Output::Append(key) => match schema.state.get(key) {
            Some(Kind::List { element }) if **element == Kind::Str => return Ok(()),
            Some(Kind::List { element }) => (key, element.as_ref()),
            Some(_) => return Err(GraphError::AppendNotList { key: key.clone() }),
            None => return Err(GraphError::UnknownKey { key: key.clone() }),
        },
    };
    Err(GraphError::KindMismatch {
        key: key.clone(),
        expected: expected.clone(),
        found: Kind::Str.tag(),
    })
}
