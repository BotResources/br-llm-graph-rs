//! What the child of a runner call answers: the blocks of its reply, in
//! order, and its thread.

use br_llm_messages::Text;
use serde_json::Value;

use crate::value::ThreadId;

/// One block of a child's reply. More kinds of block may come in a later
/// version: a `match` on it outside the crate needs a wildcard arm.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AnswerBlock {
    /// A text block.
    Text(Text),
    /// A structured value.
    Structured(Value),
}

/// The answer of the child of a runner call: the blocks of its reply, texts
/// and structured values in the order the reply holds them, and the id of
/// the child's thread.
///
/// A port builds it with `new`: it may gain fields in a later version.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct RunnerAnswer {
    /// The blocks of the reply, in order.
    pub blocks: Vec<AnswerBlock>,
    /// The thread the child answered in.
    pub thread: ThreadId,
}

impl RunnerAnswer {
    /// The answer holding `blocks`, given in `thread`.
    pub fn new(blocks: Vec<AnswerBlock>, thread: ThreadId) -> Self {
        Self { blocks, thread }
    }

    /// Its text: the JSON text of its structured block when it has one (the
    /// first), else its text blocks joined verbatim, with nothing between
    /// them; as an `LlmCall` reads a reply.
    pub fn text(&self) -> String {
        let structured = self.blocks.iter().find_map(|block| match block {
            AnswerBlock::Structured(value) => Some(value),
            AnswerBlock::Text(_) => None,
        });
        match structured {
            Some(value) => value.to_string(),
            None => self
                .blocks
                .iter()
                .filter_map(|block| match block {
                    AnswerBlock::Text(text) => Some(text.as_str()),
                    AnswerBlock::Structured(_) => None,
                })
                .collect(),
        }
    }
}
