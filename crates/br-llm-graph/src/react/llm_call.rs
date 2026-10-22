use br_llm_clients_core::{ClientRequest, OutputMode};
use br_llm_messages::{Author, Conversation, Text, WireMessage, WireUserBlock};

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Count, Node, NodeError, NodeFuture};
use crate::react::given::{CallFacts, History, Model, Prompt};
use crate::react::helpers::{answer_text, system_text, wire};
use crate::react::llm_call_check::{check_conversation, check_output};
use crate::react::replies::{Unusable, judged, reminded, reminder};
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
        /// The conversation key.
        key: StateKey<Conversation>,
        /// The author it is rendered for.
        author: Author,
    },
    /// The conversation to send, as wire messages, usually given by a
    /// function of the call's context. An empty one fails the node
    /// (`Request`, carrying `NoMessage`).
    History(History<LlmCall>),
}

/// Where the answer of an `LlmCall` goes: a string state key it sets, or a
/// list of strings it appends to.
///
/// More outputs may come in a later version: a `match` on it outside the crate
/// needs a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
/// With a structured output mode, the reply is checked against its shape
/// before its answer is written: it fits when its structured block is an
/// object that holds every required parameter and no name the shape does
/// not declare, each value of its parameter's kind. A reply cannot be used
/// when it does not fit, when it has no structured block, or, whatever the
/// mode, when the client refuses it as unusable (`ReplyUnusable`) or
/// malformed (`MalformedResponse`). Such a reply is asked for again while
/// `reply_retries` are left: the same request, built for the next attempt
/// (`RequestContext::attempt`, one higher, so a model function may give
/// another client), followed by a user message holding the `reminder`
/// (one more text of the last message when it is a relay). A reply asked
/// for again is never written. When no retry is left, the node fails with
/// `Model`, carrying the client's error, or `Structured`, carrying why the
/// reply does not fit.
///
/// The stream events of the call reach the observer with the conversation
/// key of an `LlmRequest::Conversation`, or the key of `output` for any
/// other request, which has no conversation key; the events of a reply
/// asked for again reach it too. Any other error of the client fails the
/// node with `Model`, a refusal to build the request with `Request`, and an
/// error of a function as the function returned it.
///
/// As the body of a `Map`, an `Append` output makes one model call per item,
/// at most the map's concurrency limit at once, the answers appended in item
/// order; a `Set` output is refused there (`MapBodySet`).
///
/// At build, the call checks the keys its functions declare (`UnknownKey`,
/// or `KindMismatch` when the graph declares one with another kind), its
/// conversation key (`UnknownKey`, or `KindMismatch` when it is not a
/// conversation), its output key, refused with the error writing a string
/// through it would give (`UnknownKey`, `KindMismatch`, `AppendNotList`),
/// and the key of its reply retries (`CountKeyMismatch`). A negative count
/// read at run time fails the node (`CountNegative`) before the model is
/// called.
pub struct LlmCall {
    /// The client the call calls.
    pub model: Model<LlmCall>,
    /// The system text; one that resolves blank gives none.
    pub system: Prompt<LlmCall>,
    /// What the call sends as its messages.
    pub request: LlmRequest,
    /// The output mode sent; with a structured mode, the answer is the JSON
    /// text of the reply's structured block.
    pub output_mode: OutputMode,
    /// Where the answer goes.
    pub output: LlmOutput,
    /// The times a reply that cannot be used is asked for again. `None`: 0,
    /// the node fails on the first.
    pub reply_retries: Option<Count>,
    /// The text sent after the request when a reply is asked for again, each
    /// `{reason}` in it replaced by why the reply could not be used; one
    /// that resolves blank fails the node (`Message`). `None`: `Your reply
    /// could not be used: {reason}. Reply again.`
    pub reminder: Option<Prompt<LlmCall>>,
}

impl Node for LlmCall {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let attempts = match &self.reply_retries {
                Some(retries) => retries.resolve(config)?.saturating_add(1),
                None => 1,
            };
            let mut refused: Option<Unusable> = None;
            let mut attempt = 1;
            loop {
                let facts = CallFacts { attempt };
                let (mut messages, key) = self.messages(state, config, facts)?;
                if let Some(unusable) = &refused {
                    let text = reminder(
                        self.reminder.as_ref(),
                        state,
                        config,
                        facts,
                        &unusable.reason(),
                    )?;
                    messages = reminded(messages, text);
                }
                let request = ClientRequest::new(
                    system_text(self.system.give(state, config, facts)?),
                    messages,
                    Vec::new(),
                    self.output_mode.clone(),
                )
                .map_err(GraphError::Request)?;
                let client = self.model.give(state, config, facts)?;
                let result = complete(client.as_ref(), &request, ctx, key).await;
                match judged(result, &self.output_mode)? {
                    Ok(step) => return Ok(vec![self.output.update(answer_text(&step))]),
                    Err(unusable) if attempt < attempts => {
                        refused = Some(unusable);
                        attempt += 1;
                    }
                    Err(unusable) => return Err(unusable.into_error().into()),
                }
            }
        })
    }

    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        self.model.reads().check(schema)?;
        self.system.reads().check(schema)?;
        if let Some(reminder) = &self.reminder {
            reminder.reads().check(schema)?;
        }
        if let Some(retries) = &self.reply_retries {
            retries.check(schema)?;
        }
        match &self.request {
            LlmRequest::Text(text) => text.reads().check(schema)?,
            LlmRequest::Conversation { key, .. } => check_conversation(schema, key.key())?,
            LlmRequest::History(history) => history.reads().check(schema)?,
        }
        check_output(schema, &self.output, site)
    }

    /// `llm_call`: how its subjects are given, its request and its output.
    fn describe(&self) -> crate::describe::NodeDescription {
        self.description()
    }
}

impl LlmCall {
    /// The messages of the request for the context `facts`, with the key its
    /// stream events carry.
    fn messages<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        facts: CallFacts,
    ) -> Result<(Vec<WireMessage>, &'a Key), NodeError> {
        Ok(match &self.request {
            LlmRequest::Text(text) => {
                let text = Text::new(text.give(state, config, facts)?).map_err(GraphError::from)?;
                let message = WireMessage::User {
                    content: vec![WireUserBlock::Text { text }],
                };
                (vec![message], self.output.key())
            }
            LlmRequest::Conversation { key, author } => (wire(state.get(key)?, author)?, key.key()),
            LlmRequest::History(history) => {
                (history.give(state, config, facts)?, self.output.key())
            }
        })
    }
}
