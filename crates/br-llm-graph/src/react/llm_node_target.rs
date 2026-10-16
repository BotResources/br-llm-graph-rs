//! Where the reply of an agent's model call goes: a new step of the turn
//! the agent writes, or a new turn.

use br_llm_messages::{Conversation, Turn, TurnId, TurnItem, TurnState};

use crate::error::GraphError;
use crate::graph::Context;
use crate::react::agent_call::Position;
use crate::react::given::LoopNode;
use crate::react::helpers::last_turn_by_author;
use crate::react::llm_node::LlmNode;
use crate::state::State;

/// Where the reply of a model call goes.
pub(crate) enum Target<'a> {
    /// A new step of this turn, which awaits one.
    Step(&'a Turn),
    /// The first step of a new turn under `id`; the call moves to `moved`
    /// when set.
    Open { id: TurnId, moved: Option<Position> },
}

/// The rounds `turn` has answered: its tool results items.
pub(crate) fn answered_rounds(turn: &Turn) -> usize {
    turn.items()
        .iter()
        .filter(|item| matches!(item, TurnItem::ToolResults(_)))
        .count()
}

impl<N: LoopNode> LlmNode<N> {
    /// The 0-based index of the turn the reply goes to among the call's
    /// turns, as the call ranks them: 0 for a turn the call continues in
    /// memory.
    pub(crate) fn turn_index(
        &self,
        state: &State,
        target: &Target<'_>,
    ) -> Result<usize, GraphError> {
        let rank = match (target, self.call.position(state)?) {
            (
                Target::Open {
                    moved: Some(Position::Writing(rank)),
                    ..
                },
                _,
            ) => *rank,
            (_, Position::Writing(rank) | Position::Answered(rank)) => rank,
        };
        Ok(rank.saturating_sub(1))
    }

    /// Where the reply goes. In memory, the author's last turn while it
    /// awaits a step, whatever turn it is (one the opening ends with
    /// included), as the rule of the ReAct loop says. Durable, only the turn
    /// of the call's rank, while it awaits a step: a turn of the thread that
    /// is not the call's, a repaired one included, is never continued. Else
    /// a new turn: the turn of the call's rank when it is not in the
    /// conversation yet, or the turn of the next rank once it is finished.
    pub(crate) fn target<'a>(
        &self,
        state: &State,
        ctx: &Context,
        conversation: &'a Conversation,
    ) -> Result<Target<'a>, GraphError> {
        // In memory, the rule of the ReAct loop is literal: the author's last
        // turn goes on while it awaits a step, wherever it comes from.
        let call = &self.call;
        if call.thread.is_none()
            && let Some(turn) = last_turn_by_author(conversation, &self.author)
            && matches!(turn.state(), TurnState::AwaitingStep)
        {
            return Ok(Target::Step(turn));
        }
        let rank = match call.position(state)? {
            Position::Writing(rank) | Position::Answered(rank) => rank,
        };
        let id = call.turn_id(state, ctx, rank)?;
        Ok(match conversation.turn(&id) {
            Some(turn) if matches!(turn.state(), TurnState::AwaitingStep) => Target::Step(turn),
            Some(_) => Target::Open {
                id: call.turn_id(state, ctx, rank + 1)?,
                moved: Some(Position::Writing(rank + 1)),
            },
            None => Target::Open { id, moved: None },
        })
    }
}
