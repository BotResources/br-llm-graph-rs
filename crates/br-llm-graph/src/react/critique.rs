//! The critic's step of the generator-critic: its call, asked again while
//! its verdict cannot be read and retries are left, and the acceptance or
//! the critique it records.

use br_llm_clients_core::{ClientRequest, ReasoningLevel};
use br_llm_messages::{Text, WireMessage};

use crate::error::GraphError;
use crate::graph::{Context, Count, Node, NodeFuture};
use crate::react::critic_view::CriticView;
use crate::react::critic_writer::record;
use crate::react::duo::TurnKind;
use crate::react::given::{CriticFacts, Model, Prompt};
use crate::react::helpers::{answer_text, final_step};
use crate::react::replies::{refuses, reminded};
use crate::react::seats::CriticSeat;
use crate::react::sink::complete;
use crate::react::verdict::{Reading, Verdict, VerdictForm, unreadable};
use crate::state::{Config, State};
use crate::value::Key;

/// The request that follows the wrapped answer, by default.
pub(crate) const REVIEW_REQUEST: &str = "Please evaluate the generator's response above.";

/// The acceptance recorded for a valid verdict whose message is blank.
pub(crate) const ACCEPTED_TEXT: &str = "Accepted.";

/// Asks the critic for a verdict on the last generation of the call, and
/// records it: a valid verdict as an acceptance (its message, or `Accepted.`
/// when blank, stop reason `other(accepted)`), a rejection as a critique
/// (its message, `end_turn`).
///
/// Each call is built for the critic's context, its attempt from 1: the
/// client `model` gives, the system text (always sent: a text that resolves
/// to a blank one fails the node with `Message`, before the call), what the
/// critic reads, no tools, and the structured mode with the verdict shape,
/// `thinking` in it exactly when that client's `ReasoningLevel` is `Off`.
/// Its stream events carry the key `critic`, and reach no thread.
///
/// An unreadable verdict, a reply the client refuses as unusable or
/// malformed included, is asked again while `retries` are left: the same
/// request for the next attempt, then a user message saying why the reply
/// could not be read (one more text of the last message when it is a
/// relay). After the last retry the node fails with `Structured`,
/// and nothing is recorded; a retry is never recorded.
pub(crate) struct CritiqueNode {
    pub(crate) model: Model<CriticSeat>,
    pub(crate) system: Prompt<CriticSeat>,
    pub(crate) view: CriticView,
    pub(crate) verdict: VerdictForm,
    pub(crate) retries: Count,
    /// The key the critic's stream events carry: `critic`.
    pub(crate) stream_key: Key,
}

impl Node for CritiqueNode {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let duo = &self.view.duo;
            let turns = duo.turns(state)?;
            let Some(judged) = turns.last_generation() else {
                return Ok(Vec::new());
            };
            let answer = final_step(judged).map(answer_text).unwrap_or_default();
            let count = duo.count(config, &turns, turns.generations())?;
            let conversation = state.conversation(&duo.conversation)?;
            let attempts = self.retries.resolve(config)?.saturating_add(1);
            let mut why = String::new();
            for attempt in 1..=attempts {
                let facts = CriticFacts {
                    conversation,
                    count,
                    answer: &answer,
                    attempt,
                };
                let mut messages = self.view.messages(state, config, facts, judged)?;
                if attempt > 1 {
                    messages = reminded(messages, Some(retry_text(&why)?));
                }
                match self.ask(state, config, ctx, facts, messages).await? {
                    Reading::Verdict(Verdict {
                        is_valid: true,
                        message,
                    }) => {
                        let text = Text::new(message).or_else(|_| Text::new(ACCEPTED_TEXT))?;
                        return Ok(record(duo, state, ctx, TurnKind::Acceptance, text).await?);
                    }
                    Reading::Verdict(Verdict { message, .. }) => {
                        let text = Text::new(message)?;
                        return Ok(record(duo, state, ctx, TurnKind::Critique, text).await?);
                    }
                    Reading::Unreadable { reason } => why = reason,
                }
            }
            Err(GraphError::Structured { message: why }.into())
        })
    }

    /// `model_call`: the critic's seat and what the routing and the verdict read.
    fn describe(&self) -> crate::describe::NodeDescription {
        self.description()
    }
}

impl CritiqueNode {
    /// One call of the critic for its context `facts`, sending `messages`:
    /// what it gives, read as a verdict. A reply the client refuses as
    /// unusable or malformed is an unreadable verdict, its reason the
    /// client's error.
    async fn ask(
        &self,
        state: &State,
        config: &Config,
        ctx: &Context,
        facts: CriticFacts<'_>,
        messages: Vec<WireMessage>,
    ) -> Result<Reading, crate::graph::NodeError> {
        let client = self.model.give(state, config, facts)?;
        let system =
            Text::new(self.system.give(state, config, facts)?).map_err(GraphError::from)?;
        let with_thinking = client.settings().reasoning() == ReasoningLevel::Off;
        let request = ClientRequest::new(
            Some(system),
            messages,
            Vec::new(),
            self.verdict
                .output_mode(state, config, facts, with_thinking)?,
        )
        .map_err(GraphError::Request)?;
        let step = match complete(client.as_ref(), &request, ctx, &self.stream_key).await {
            Ok(step) => step,
            Err(GraphError::Model(error)) if refuses(&error) => {
                return Ok(unreadable(&error.to_string()));
            }
            Err(error) => return Err(error.into()),
        };
        Ok(self.verdict.read(&step)?)
    }
}

/// The text that asks the critic again after an unreadable verdict.
fn retry_text(reason: &str) -> Result<Text, GraphError> {
    Ok(Text::new(format!(
        "Your reply could not be read as a verdict: {reason}. Reply again with the verdict."
    ))?)
}
