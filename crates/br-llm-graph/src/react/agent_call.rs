//! What the nodes of an agent's graph share about the call they serve: the
//! keys of the agent's own state, its thread store when it is durable, and
//! where the call stands among its turns.

use std::sync::Arc;

use br_llm_messages::{InputId, TurnId, UserInput};

use crate::error::GraphError;
use crate::graph::Context;
use crate::react::thread_store::ThreadStore;
use crate::react::turn_ids::{id_request, trigger};
use crate::state::{State, Value};
use crate::value::{Key, ThreadId};

/// A durable agent's thread: its store and the state key holding its id.
#[derive(Clone)]
pub(crate) struct ThreadBinding {
    pub(crate) store: Arc<dyn ThreadStore>,
    /// The `thread` key: the id given (empty to ask for one), then the id of
    /// the thread the agent works in once it has entered.
    pub(crate) key: Key,
}

impl ThreadBinding {
    /// The thread the agent works in, as its entry wrote it into `key`.
    pub(crate) fn thread(&self, state: &State) -> Result<ThreadId, GraphError> {
        ThreadId::new(state.str(&self.key)?)
    }
}

/// The call of an agent, as its nodes find it in the state.
#[derive(Clone)]
pub(crate) struct AgentCall {
    /// The `opening` key: the trigger of the call is its last user input.
    pub(crate) opening: Key,
    /// The private key holding where the call stands (`Position`).
    pub(crate) position: Key,
    /// The agent's thread, when it is durable.
    pub(crate) thread: Option<ThreadBinding>,
}

/// Where the call of an agent stands among its turns, the ranks counted as
/// the id source counts them (1 for the first turn of the call).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Position {
    /// The agent writes the turn of this rank: it opens it at its next model
    /// call when it is not in the conversation yet, else goes on in it while
    /// it awaits a step.
    Writing(usize),
    /// The call was answered on entry, by the completed turn of this rank:
    /// the agent gives its outputs without calling a model.
    Answered(usize),
}

impl Position {
    /// The value of the private key: the rank, negated when answered.
    pub(crate) fn value(self) -> Value {
        let signed = |rank: usize| i64::try_from(rank).unwrap_or(i64::MAX);
        match self {
            Position::Writing(rank) => Value::int(signed(rank)),
            Position::Answered(rank) => Value::int(-signed(rank)),
        }
    }

    fn from_int(value: i64) -> Self {
        let rank = usize::try_from(value.unsigned_abs()).unwrap_or(usize::MAX);
        if value < 0 {
            Position::Answered(rank)
        } else {
            Position::Writing(rank)
        }
    }
}

impl AgentCall {
    /// Where the call stands.
    pub(crate) fn position(&self, state: &State) -> Result<Position, GraphError> {
        Ok(Position::from_int(state.int(&self.position)?))
    }

    /// The update that moves the call to `position`.
    pub(crate) fn move_to(&self, position: Position) -> crate::update::Update {
        crate::update::Update::Set {
            key: self.position.clone(),
            value: position.value(),
        }
    }

    /// The id of the user input the call answers: the last user input of
    /// `opening`, if any.
    pub(crate) fn trigger<'a>(&self, state: &'a State) -> Result<Option<&'a InputId>, GraphError> {
        Ok(trigger(state.conversation(&self.opening)?).map(UserInput::id))
    }

    /// The id of the turn of rank `rank` of this call.
    pub(crate) fn turn_id(
        &self,
        state: &State,
        ctx: &Context,
        rank: usize,
    ) -> Result<TurnId, GraphError> {
        Ok(ctx
            .ids
            .turn_id(&id_request(ctx, self.trigger(state)?, rank)))
    }
}
