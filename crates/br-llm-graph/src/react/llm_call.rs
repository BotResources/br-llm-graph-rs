use br_llm_clients_core::{ClientRequest, OutputMode};
use br_llm_messages::{Author, Conversation, Text, WireMessage, WireUserBlock};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeError, NodeFuture};
use crate::react::given::{History, Model, Prompt};
use crate::react::helpers::{answer_text, system_text, wire};
use crate::react::llm_call_check::{check_conversation, check_output};
use crate::react::sink::complete;
use crate::state::{Config, Schema, State, StateKey, Value};
use crate::update::Update;
use crate::value::Key;

/// What an `LlmCall` sends as its request. More kinds of request may come in
/// a later version: a `match` on it outside the crate needs a wildcard arm.
#[derive(Clone)]
#[non_exhaustive]
pub enum LlmRequest {
    /// A text, sent as one user message holding it. A text that resolves
    /// blank fails the node (`Message`: a message cannot hold a blank text).
    Text(Prompt<LlmCall>),
    /// A conversation state key, rendered for `author`: for images or
    /// earlier turns. An empty conversation fails the node (`Message`, the
    /// render's refusal). The call declares no tools, so tool calls and tool
    /// results in this conversation reach the client with no tool declared,
    /// which some providers (Anthropic, Bedrock) refuse: that is a matter of
    /// the provider's table, so keep tool traffic out of an `LlmCall`'s
    /// conversation.
    Conversation {
        key: StateKey<Conversation>,
        author: Author,
    },
    /// The conversation to send, as wire messages, usually given by a
    /// function of the call's context. An empty one fails the node
    /// (`Request`, carrying `NoMessage`).
    History(History<LlmCall>),
}

/// Where the answer of an `LlmCall` goes: a string state key it sets, or a
/// list of strings it appends to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmOutput {
    /// Sets a string state key to the answer.
    Set(StateKey<String>),
    /// Appends the answer to a list of strings.
    Append(StateKey<Vec<String>>),
}

impl LlmOutput {
    /// The key the answer goes to.
    pub fn key(&self) -> &Key {
        match self {
            LlmOutput::Set(key) => key.key(),
            LlmOutput::Append(key) => key.key(),
        }
    }

    fn update(&self, answer: String) -> Update {
        match self {
            LlmOutput::Set(key) => Update::Set {
                key: key.key().clone(),
                value: Value::str(answer),
            },
            LlmOutput::Append(key) => Update::Append {
                key: key.key().clone(),
                value: Value::str(answer),
            },
        }
    }
}

/// A node that makes one model call and gives back its answer as data.
///
/// It is built from what the runner gives it (see `Given`), each subject a
/// value or a function of the call's context, which holds the state and the
/// configuration: `model` gives the client to call, `system` the system text
/// (one that resolves blank gives none), `request` the messages. The request
/// declares no tools: a call cannot have tools, and it may ask for a
/// structured reply through `output_mode`.
///
/// The answer is the JSON text of the reply's structured block when it has
/// one, else its text blocks joined verbatim; it is written through
/// `output`. Nothing else is written: the call keeps no conversation.
///
/// The stream events of the call reach the observer with the conversation
/// key of an `LlmRequest::Conversation`, or the key of `output` for any
/// other request, which has no conversation key. An error of the client
/// fails the node with `Model`, a refusal to build the request with
/// `Request`, and an error of a function as the function returned it.
///
/// As the body of a `Map`, an `Append` output makes one model call per item,
/// at most the map's concurrency limit at once, the answers appended in item
/// order; a `Set` output is refused there (`MapBodySet`).
///
/// At build, the call checks the keys its functions declare (`UnknownKey`,
/// or `KindMismatch` when the graph declares one with another kind), its
/// conversation key (`UnknownKey`, or `KindMismatch` when it is not a
/// conversation) and its output key, refused with the error writing a string
/// through it would give (`UnknownKey`, `KindMismatch`, `AppendNotList`).
pub struct LlmCall {
    /// The client the call calls.
    pub model: Model<LlmCall>,
    /// The system text; one that resolves blank gives none.
    pub system: Prompt<LlmCall>,
    pub request: LlmRequest,
    pub output_mode: OutputMode,
    /// Where the answer goes.
    pub output: LlmOutput,
}

impl Node for LlmCall {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let (messages, key) = self.messages(state, config)?;
            let request = ClientRequest::new(
                system_text(self.system.give(state, config, ())?),
                messages,
                Vec::new(),
                self.output_mode.clone(),
            )
            .map_err(GraphError::Request)?;
            let client = self.model.give(state, config, ())?;
            let step = complete(client.as_ref(), &request, ctx, key).await?;
            Ok(vec![self.output.update(answer_text(&step))])
        })
    }

    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        self.model.reads().check(schema)?;
        self.system.reads().check(schema)?;
        match &self.request {
            LlmRequest::Text(text) => text.reads().check(schema)?,
            LlmRequest::Conversation { key, .. } => check_conversation(schema, key.key())?,
            LlmRequest::History(history) => history.reads().check(schema)?,
        }
        check_output(schema, &self.output, site)
    }
}

impl LlmCall {
    /// The messages of the request, with the key its stream events carry.
    fn messages<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
    ) -> Result<(Vec<WireMessage>, &'a Key), NodeError> {
        Ok(match &self.request {
            LlmRequest::Text(text) => {
                let text = Text::new(text.give(state, config, ())?).map_err(GraphError::from)?;
                let message = WireMessage::User {
                    content: vec![WireUserBlock::Text { text }],
                };
                (vec![message], self.output.key())
            }
            LlmRequest::Conversation { key, author } => (wire(state.get(key)?, author)?, key.key()),
            LlmRequest::History(history) => (history.give(state, config, ())?, self.output.key()),
        })
    }
}
