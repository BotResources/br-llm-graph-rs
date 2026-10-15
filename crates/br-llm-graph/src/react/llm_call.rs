use std::sync::Arc;

use br_llm_clients_core::{Client, ClientRequest, OutputMode};
use br_llm_messages::{Author, Text, WireMessage, WireUserBlock};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture, Output};
use crate::react::helpers::{answer_text, complete, system_text, wire};
use crate::react::source::Source;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::Key;

/// What an `LlmCall` sends as its request. More kinds of request may come in
/// a later version: a `match` on it outside the crate needs a wildcard arm.
#[derive(Clone)]
#[non_exhaustive]
pub enum LlmRequest {
    /// A text, sent as one user message holding it. A text that resolves
    /// blank fails the node (`Message`: a message cannot hold a blank text).
    Text(Source),
    /// A conversation state key, rendered for `author`: for images or
    /// earlier turns. An empty conversation fails the node (`Message`, the
    /// render's refusal). The call declares no tools, so tool calls and tool
    /// results in this history reach the client with no tool declared, which
    /// some providers (Anthropic, Bedrock) refuse: that is a matter of the
    /// provider's table, so keep tool traffic out of an `LlmCall`'s
    /// conversation.
    Conversation { key: Key, author: Author },
}

/// A node that makes one model call and gives back its answer as data.
///
/// The request holds the text of `system` (a blank text gives no system
/// text), the messages of `request`, no tools and `output_mode`: a call can
/// not have tools, and it may ask for a structured reply. The answer is the
/// JSON text of the reply's structured block when it has one, else its text
/// blocks joined verbatim; it is written through `output`, `Set` to a string
/// state key or `Append` to a list of strings. Nothing else is written: the
/// call keeps no conversation.
///
/// The stream events of the call reach the observer with the conversation
/// key of an `LlmRequest::Conversation`, or the key of `output` for an
/// `LlmRequest::Text`, which has no conversation key. A refusal to build the
/// request fails the node with `Request`, an error of the client with
/// `Model`.
///
/// As the body of a `Map`, an `Append` output makes one model call per item,
/// at most the map's concurrency limit at once, the answers appended in item
/// order; a `Set` output is refused there (`MapBodySet`).
///
/// At build, the call checks the keys its sources read (`SourceKeyMismatch`),
/// its conversation key (`UnknownKey`, or `KindMismatch` when it is not a
/// conversation) and its output key, refused with the error writing a string
/// through it would give (`UnknownKey`, `KindMismatch`, `AppendNotList`).
pub struct LlmCall {
    pub client: Arc<dyn Client>,
    /// The system text. A source that resolves blank, such as
    /// `Source::Const(String::new())`, gives none.
    pub system: Source,
    pub request: LlmRequest,
    pub output_mode: OutputMode,
    /// Where the answer goes.
    pub output: Output,
}

impl Node for LlmCall {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let (messages, key) = match &self.request {
                LlmRequest::Text(source) => {
                    let text =
                        Text::new(source.resolve(state, config)?).map_err(GraphError::from)?;
                    let message = WireMessage::User {
                        content: vec![WireUserBlock::Text { text }],
                    };
                    (vec![message], self.output.key())
                }
                LlmRequest::Conversation { key, author } => {
                    (wire(state.conversation(key)?, author)?, key)
                }
            };
            let request = ClientRequest::new(
                system_text(self.system.resolve(state, config)?),
                messages,
                Vec::new(),
                self.output_mode.clone(),
            )
            .map_err(GraphError::Request)?;
            let step = complete(self.client.as_ref(), &request, ctx, key).await?;
            Ok(vec![self.output.update(Value::str(answer_text(&step)))])
        })
    }

    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        self.system.check(schema)?;
        match &self.request {
            LlmRequest::Text(source) => source.check(schema)?,
            LlmRequest::Conversation { key, .. } => check_conversation(schema, key)?,
        }
        check_output(schema, &self.output, site)
    }
}

/// A conversation key must be a conversation state key: the error a read of
/// it would give otherwise.
fn check_conversation(schema: &Schema, key: &Key) -> Result<(), GraphError> {
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
fn check_output(schema: &Schema, output: &Output, site: CheckSite) -> Result<(), GraphError> {
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
