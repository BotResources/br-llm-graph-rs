//! What the critic reads by default: the conversation before the answer it
//! judges (`History`), up to the trigger (`Latest`) or a brief (`Brief`),
//! then the answer, wrapped, and the review request.

use br_llm_messages::{
    AssistantBlock, Conversation, Entry, Perspective, Step, StopReason, Text, Turn, WireMessage,
    WireUserBlock, render,
};

use crate::error::GraphError;
use crate::graph::NodeError;
use crate::react::duo::Duo;
use crate::react::given::{CriticFacts, Prompt};
use crate::react::helpers::final_step;
use crate::react::seats::{CriticHistory, CriticSeat};
use crate::state::{Config, State};

/// What the critic reads, and the request that follows the answer.
pub(crate) struct CriticView {
    pub(crate) history: CriticHistory,
    pub(crate) review_request: Prompt<CriticSeat>,
    pub(crate) duo: Duo,
}

impl CriticView {
    /// The messages of the critic's request for its context `facts`,
    /// judging the generation `judged` of its conversation: a function gives
    /// them all; each default ends with the answer, wrapped, followed by the
    /// review request.
    pub(crate) fn messages(
        &self,
        state: &State,
        config: &Config,
        facts: CriticFacts<'_>,
        judged: &Turn,
    ) -> Result<Vec<WireMessage>, NodeError> {
        let duo = &self.duo;
        let conversation = facts.conversation;
        let mut messages = match &self.history {
            CriticHistory::Given(history) => return history.give(state, config, facts),
            CriticHistory::History => {
                let end = position(conversation, judged.id().as_str());
                as_the_generator_sees(duo, conversation.entries().get(..end))?
            }
            CriticHistory::Latest => {
                let trigger = duo.call.trigger(state)?;
                let end = trigger.map_or(0, |trigger| position(conversation, trigger.as_str()) + 1);
                as_the_generator_sees(duo, conversation.entries().get(..end))?
            }
            CriticHistory::Brief(key) => {
                let brief = state.get(key)?;
                if brief.entries().is_empty() {
                    Vec::new()
                } else {
                    render(brief, &Perspective::new(duo.critic.clone()))
                        .map_err(GraphError::from)?
                }
            }
        };
        let request = self.review_request.give(state, config, facts)?;
        messages.push(wrapped(facts.answer, &request)?);
        Ok(messages)
    }
}

/// The position of the entry `id` in `conversation`; its length when it is
/// not there.
fn position(conversation: &Conversation, id: &str) -> usize {
    conversation
        .entries()
        .iter()
        .position(|entry| entry.id() == id)
        .unwrap_or(conversation.entries().len())
}

/// `entries` rendered as the generator sees them, each generator turn
/// reduced to its final step without thinking and tool calls: an answer left
/// with nothing, and a generation the round limit cut, are dropped, so the
/// user messages around them merge.
fn as_the_generator_sees(
    duo: &Duo,
    entries: Option<&[Entry]>,
) -> Result<Vec<WireMessage>, GraphError> {
    let mut seen = Conversation::new();
    for entry in entries.unwrap_or_default() {
        match entry {
            Entry::UserInput(input) => seen.push_input(input.clone())?,
            Entry::Turn(turn) if turn.author() == Some(&duo.generator) => {
                if let Some(answer) = reduced(duo, turn)? {
                    seen.push_turn(answer)?;
                }
            }
            Entry::Turn(turn) => seen.push_turn(turn.clone())?,
        }
    }
    if seen.entries().is_empty() {
        return Ok(Vec::new());
    }
    render(&seen, &Perspective::new(duo.generator.clone())).map_err(GraphError::from)
}

/// The generator's turn `turn` reduced to its final step, without its
/// thinking and its tool calls; `None` when nothing is left, or when the
/// round limit cut it.
fn reduced(duo: &Duo, turn: &Turn) -> Result<Option<Turn>, GraphError> {
    if duo.cut(turn) {
        return Ok(None);
    }
    let Some(step) = final_step(turn) else {
        return Ok(None);
    };
    let blocks: Vec<AssistantBlock> = step
        .content()
        .iter()
        .filter(|block| {
            matches!(
                block,
                AssistantBlock::Text { .. } | AssistantBlock::Structured { .. }
            )
        })
        .cloned()
        .collect();
    if blocks.is_empty() {
        return Ok(None);
    }
    let step = Step::new(blocks, StopReason::EndTurn, None, None)?;
    Ok(Some(Turn::new(
        turn.id().clone(),
        turn.author().cloned(),
        step,
    )))
}

/// The answer as one user message: `<generator_response>`, a line break,
/// the answer, a line break, `</generator_response>`, then a blank line and
/// `request`, both left out when `request` is blank.
fn wrapped(answer: &str, request: &str) -> Result<WireMessage, GraphError> {
    let mut text = format!("<generator_response>\n{answer}\n</generator_response>");
    if !request.trim().is_empty() {
        text.push_str("\n\n");
        text.push_str(request);
    }
    Ok(WireMessage::User {
        content: vec![WireUserBlock::Text {
            text: Text::new(text)?,
        }],
    })
}
