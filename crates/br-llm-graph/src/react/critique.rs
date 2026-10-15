//! The critic's step of the generator-critic graph: what the critic reads,
//! its call, and the critique it appends.

use std::sync::Arc;

use br_llm_clients_core::{Client, ClientRequest};
use br_llm_messages::{
    AssistantBlock, Author, Conversation, Entry, Step, StopReason, Text, Turn, WireMessage,
    WireUserBlock,
};

use crate::error::GraphError;
use crate::graph::{Context, Node, NodeFuture};
use crate::react::generator_critic_routing::last_answer;
use crate::react::helpers::{answer_text, complete, final_step, system_text, wire};
use crate::react::round_limit::closes_on_limit;
use crate::react::source::Source;
use crate::react::verdict::{Verdict, VerdictForm};
use crate::state::{Config, State, Value};
use crate::update::Update;
use crate::value::Key;

/// The request that follows the wrapped answer, by default.
pub(crate) const REVIEW_REQUEST: &str = "Please evaluate the generator's response above.";

/// The critique that rejects an empty answer, by default.
pub(crate) const EMPTY_ANSWER_CRITIQUE: &str = "The answer was empty. Write a complete answer.";

/// Asks the critic for a verdict on `conversation`, or rejects an empty
/// answer without asking. A rejection appends its message, as a turn of the
/// critic, to both histories.
pub(crate) struct CritiqueNode {
    pub(crate) conversation: Key,
    pub(crate) generator: Author,
    pub(crate) generator_history: Key,
    pub(crate) validated: Key,
    pub(crate) author: Author,
    pub(crate) client: Arc<dyn Client>,
    pub(crate) system: Source,
    pub(crate) review_request: Source,
    pub(crate) empty_answer_critique: Source,
    pub(crate) verdict: VerdictForm,
}

impl Node for CritiqueNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.conversation)?;
            let verdict = match last_answer(conversation, &self.generator) {
                Some((index, step)) if !empty_step(step) => {
                    let messages = self.messages(state, config, conversation, index, step)?;
                    self.judge(state, config, ctx, messages).await?
                }
                Some(_) | None => Verdict {
                    is_valid: false,
                    message: self.empty_answer_text(state, config)?,
                },
            };
            if verdict.is_valid {
                return Ok(vec![Update::Set {
                    key: self.validated.clone(),
                    value: Value::bool(true),
                }]);
            }
            let critique = Step::new(
                vec![AssistantBlock::Text {
                    text: Text::new(verdict.message)?,
                }],
                StopReason::EndTurn,
                None,
                None,
            )?;
            let turn = Turn::new(ctx.ids.turn_id(), Some(self.author.clone()), critique);
            Ok(vec![
                Update::Set {
                    key: self.validated.clone(),
                    value: Value::bool(false),
                },
                Update::PushTurn {
                    key: self.conversation.clone(),
                    turn: turn.clone(),
                },
                Update::PushTurn {
                    key: self.generator_history.clone(),
                    turn,
                },
            ])
        })
    }
}

impl CritiqueNode {
    /// The critique of an empty answer: `empty_answer_critique`, or the
    /// default text when it resolves to a blank text.
    fn empty_answer_text(&self, state: &State, config: &Config) -> Result<String, GraphError> {
        let text = self.empty_answer_critique.resolve(state, config)?;
        if text.trim().is_empty() {
            Ok(EMPTY_ANSWER_CRITIQUE.to_owned())
        } else {
            Ok(text)
        }
    }

    /// What the critic reads: `conversation` before the answer at `index`,
    /// rendered as the generator sees it with the generator's thinking
    /// removed, then one user message holding the answer, wrapped, and the
    /// review request. A generation the round limit cut is an empty answer:
    /// it is left out, like an answer left with nothing once its thinking is
    /// removed, and the user messages around it are merged.
    fn messages(
        &self,
        state: &State,
        config: &Config,
        conversation: &Conversation,
        index: usize,
        answer: &Step,
    ) -> Result<Vec<WireMessage>, GraphError> {
        let mut earlier = Conversation::new();
        for entry in conversation.entries().get(..index).unwrap_or_default() {
            match entry {
                Entry::UserInput(input) => earlier.push_input(input.clone()),
                Entry::Turn(turn) if cut_generation(turn, &self.generator) => {}
                Entry::Turn(turn) => earlier.push_turn(turn.clone())?,
            }
        }
        let mut messages = if earlier.entries().is_empty() {
            Vec::new()
        } else {
            without_thinking(wire(&earlier, &self.generator)?)
        };
        let mut text = format!(
            "<generator_response>\n{}\n</generator_response>",
            answer_text(answer)
        );
        let request = self.review_request.resolve(state, config)?;
        if !request.trim().is_empty() {
            text.push_str("\n\n");
            text.push_str(&request);
        }
        messages.push(WireMessage::User {
            content: vec![WireUserBlock::Text {
                text: Text::new(text)?,
            }],
        });
        Ok(messages)
    }

    /// The critic's call: its system text, `messages`, no tools and the
    /// structured mode with the verdict shape. Its stream events carry the
    /// key `conversation`.
    async fn judge(
        &self,
        state: &State,
        config: &Config,
        ctx: &Context,
        messages: Vec<WireMessage>,
    ) -> Result<Verdict, GraphError> {
        let request = ClientRequest::new(
            system_text(self.system.resolve(state, config)?),
            messages,
            Vec::new(),
            self.verdict.output_mode(state, config)?,
        )
        .map_err(GraphError::Request)?;
        let step = complete(self.client.as_ref(), &request, ctx, &self.conversation).await?;
        self.verdict.read(&step)
    }
}

/// Whether `turn` is a generation of `generator` that the round limit cut
/// (`OnLimit::End`'s closing step as its final step).
fn cut_generation(turn: &Turn, generator: &Author) -> bool {
    turn.author() == Some(generator) && final_step(turn).is_some_and(closes_on_limit)
}

/// Whether an answer is empty: no non-blank text and no structured block,
/// or a generation the round limit cut (`OnLimit::End`'s closing step).
fn empty_step(step: &Step) -> bool {
    closes_on_limit(step)
        || (step.structured().next().is_none()
            && step.text().all(|text| text.as_str().trim().is_empty()))
}

/// Removes the thinking blocks of every assistant message. A message left
/// with nothing is dropped, and the user messages it separated are merged,
/// as `render` merges adjacent user messages.
fn without_thinking(messages: Vec<WireMessage>) -> Vec<WireMessage> {
    let mut kept: Vec<WireMessage> = Vec::new();
    for message in messages {
        let message = match message {
            WireMessage::Assistant { content } => {
                let content: Vec<AssistantBlock> = content
                    .into_iter()
                    .filter(|block| {
                        !matches!(
                            block,
                            AssistantBlock::Thinking(_) | AssistantBlock::RedactedThinking(_)
                        )
                    })
                    .collect();
                if content.is_empty() {
                    continue;
                }
                WireMessage::Assistant { content }
            }
            other @ (WireMessage::User { .. } | WireMessage::Relay { .. }) => other,
        };
        if let WireMessage::User { content: more } = &message
            && let Some(WireMessage::User { content }) = kept.last_mut()
        {
            content.extend(more.iter().cloned());
            continue;
        }
        kept.push(message);
    }
    kept
}
