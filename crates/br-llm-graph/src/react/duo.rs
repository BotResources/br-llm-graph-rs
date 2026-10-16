//! What the nodes of a generator-critic share about the call they serve:
//! its turns, each told apart by its kind, and where the call stands.

use br_llm_messages::{Author, Step, StopReason, Turn, TurnId};

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::agent_call::AgentCall;
use crate::react::given::SeatCount;
use crate::react::helpers::final_step;
use crate::react::round_limit::cut_by_limit;
use crate::state::{Config, State, Value};
use crate::value::Key;

/// The stop reason of a revision the rule check asks for.
pub(crate) const RULE_CHECK: &str = "rule_check";

/// The stop reason of an acceptance of the critic.
pub(crate) const ACCEPTED: &str = "accepted";

/// What a turn of the call is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TurnKind {
    /// A turn of the generator.
    Generation,
    /// A message of the critic rejecting an answer, or an empty one
    /// (`end_turn`).
    Critique,
    /// A message of the critic asked by the rule check (`other(rule_check)`).
    Revision,
    /// A message of the critic accepting an answer (`other(accepted)`).
    Acceptance,
}

impl TurnKind {
    /// The stop reason of a message of the critic of this kind.
    pub(crate) fn stop_reason(self) -> StopReason {
        match self {
            TurnKind::Generation | TurnKind::Critique => StopReason::EndTurn,
            TurnKind::Revision => StopReason::Other {
                reason: RULE_CHECK.to_owned(),
            },
            TurnKind::Acceptance => StopReason::Other {
                reason: ACCEPTED.to_owned(),
            },
        }
    }
}

/// The turns of one call of the generator-critic, in order, each with its
/// kind.
pub(crate) struct CallTurns<'a> {
    turns: Vec<(&'a Turn, TurnKind)>,
}

impl<'a> CallTurns<'a> {
    /// The last turn of the call, with its kind.
    pub(crate) fn last(&self) -> Option<(&'a Turn, TurnKind)> {
        self.turns.last().copied()
    }

    fn count(&self, kind: TurnKind) -> usize {
        self.turns
            .iter()
            .filter(|(_, found)| *found == kind)
            .count()
    }

    /// The critiques of the call: revisions are not counted.
    pub(crate) fn critiques(&self) -> usize {
        self.count(TurnKind::Critique)
    }

    /// The generations of the call: the generator's turns, revisions
    /// included.
    pub(crate) fn generations(&self) -> usize {
        self.count(TurnKind::Generation)
    }

    /// The generations of the call but the turn `writing`, which the
    /// generator is writing.
    pub(crate) fn generations_before(&self, writing: Option<&TurnId>) -> usize {
        self.turns
            .iter()
            .filter(|(turn, kind)| *kind == TurnKind::Generation && Some(turn.id()) != writing)
            .count()
    }

    /// The revisions in a row: those since the last critique of the call.
    pub(crate) fn revisions_in_a_row(&self) -> usize {
        self.turns
            .iter()
            .rev()
            .take_while(|(_, kind)| *kind != TurnKind::Critique)
            .filter(|(_, kind)| *kind == TurnKind::Revision)
            .count()
    }

    /// The generations of the call, in order.
    pub(crate) fn generation_turns(&self) -> impl Iterator<Item = &'a Turn> + '_ {
        self.turns
            .iter()
            .filter(|(_, kind)| *kind == TurnKind::Generation)
            .map(|(turn, _)| *turn)
    }

    /// The last generation of the call.
    pub(crate) fn last_generation(&self) -> Option<&'a Turn> {
        self.turns
            .iter()
            .rev()
            .find(|(_, kind)| *kind == TurnKind::Generation)
            .map(|(turn, _)| *turn)
    }
}

/// A generator-critic as its nodes find it.
#[derive(Clone)]
pub(crate) struct Duo {
    /// The call: its `opening`, where it stands among its turns, its thread
    /// when durable, and the private key listing its turns.
    pub(crate) call: AgentCall,
    /// The private key `conversation`.
    pub(crate) conversation: Key,
    /// The private key listing the ids of the call's turns.
    pub(crate) turns: Key,
    pub(crate) generator: Author,
    pub(crate) critic: Author,
    pub(crate) budget: Count,
    pub(crate) final_generation: bool,
    pub(crate) revision_limit: Count,
    /// Whether the generator's round limit is `OnLimit::End`, the only one
    /// that cuts a generation.
    pub(crate) limit_ends: bool,
}

impl Duo {
    /// The turns of the call in `state`, as its private key lists them, in
    /// the conversation; another author's turn is not one of them.
    pub(crate) fn turns<'a>(&self, state: &'a State) -> Result<CallTurns<'a>, GraphError> {
        let conversation = state.conversation(&self.conversation)?;
        let mut turns = Vec::new();
        for value in state.list(&self.turns)? {
            let Value::Str(id) = value else {
                continue;
            };
            let Some(turn) = conversation.turn(&TurnId::new(id.as_str())?) else {
                continue;
            };
            if let Some(kind) = self.kind(turn) {
                turns.push((turn, kind));
            }
        }
        Ok(CallTurns { turns })
    }

    /// The kind of `turn`: a turn of the generator is a generation; a turn
    /// of the critic is told by the stop reason of its final step; any other
    /// turn is not the call's.
    pub(crate) fn kind(&self, turn: &Turn) -> Option<TurnKind> {
        let author = turn.author()?;
        if *author == self.generator {
            return Some(TurnKind::Generation);
        }
        if *author != self.critic {
            return None;
        }
        Some(match final_step(turn).map(Step::stop_reason) {
            Some(StopReason::Other { reason }) if reason == RULE_CHECK => TurnKind::Revision,
            Some(StopReason::Other { reason }) if reason == ACCEPTED => TurnKind::Acceptance,
            Some(_) | None => TurnKind::Critique,
        })
    }

    /// Whether `turn` is a generation the round limit cut.
    pub(crate) fn cut(&self, turn: &Turn) -> bool {
        self.limit_ends && turn.author() == Some(&self.generator) && cut_by_limit(turn)
    }

    /// Whether the generation `turn` is an empty answer: its final step holds
    /// no non-blank text and no structured block, or the round limit cut it.
    pub(crate) fn empty(&self, turn: &Turn) -> bool {
        self.cut(turn)
            || final_step(turn).is_none_or(|step| {
                step.structured().next().is_none()
                    && step.text().all(|text| text.as_str().trim().is_empty())
            })
    }

    /// Where the call stands for its context, at the generation number
    /// `generation`.
    pub(crate) fn count(
        &self,
        config: &Config,
        turns: &CallTurns<'_>,
        generation: usize,
    ) -> Result<SeatCount, GraphError> {
        Ok(SeatCount {
            generation,
            budget: self.budget.resolve(config)?,
            critiques: turns.critiques(),
            final_generation: self.final_generation,
        })
    }
}
