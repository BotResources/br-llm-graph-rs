//! The client of each turn of an agent: asked of its model at the turn's
//! first round, kept for the turn's other rounds, so a turn is run by one
//! client.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_llm_clients_core::Client;
use br_llm_messages::TurnId;

use crate::graph::Context;
use crate::origin::Origin;
use crate::react::turn_ids::agent_origin;

/// The client kept for one call: the turn it runs, and its client.
type Kept = HashMap<Origin, (TurnId, Arc<dyn Client>)>;

/// The client kept for the turn an agent's call is writing: a run slot (see
/// `Context::run_local`) of the run of the agent's graph, so it lives in the
/// call and dies with it, however the call ends (finished, failed,
/// cancelled, or paused and never resumed). Two calls never share one, nor
/// two runs of one built graph, whatever their run ids, nor the items of a
/// map, each of which calls the agent's graph in a run of its own.
///
/// Within the run, it is kept under the agent's origin for one turn at a
/// time, and found only for that turn: the turn's end lets it go, and a new
/// turn (after a turn cut by the round limit) asks the model again. A call
/// run again is a new run: its first round asks the model again, even on a
/// turn it continues (an own turn the opening of an agent in memory ends
/// with, awaiting a step).
#[derive(Default)]
pub(crate) struct TurnClients {
    kept: Mutex<Kept>,
}

impl TurnClients {
    /// The clients of the run `ctx` belongs to.
    pub(crate) fn of(ctx: &Context) -> Arc<Self> {
        ctx.run_local()
    }

    fn lock(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The client kept for the turn `turn` of the call `ctx`'s node belongs
    /// to.
    pub(crate) fn kept(&self, ctx: &Context, turn: &TurnId) -> Option<Arc<dyn Client>> {
        self.lock()
            .get(&agent_origin(ctx))
            .filter(|(kept, _)| kept == turn)
            .map(|(_, client)| client.clone())
    }

    /// Keeps `client` for the turn `turn` of the call `ctx`'s node belongs
    /// to, in place of the client of its earlier turn.
    pub(crate) fn keep(&self, ctx: &Context, turn: TurnId, client: Arc<dyn Client>) {
        self.lock().insert(agent_origin(ctx), (turn, client));
    }

    /// Lets go of the client of the call `ctx`'s node belongs to: its turn
    /// has ended.
    pub(crate) fn forget(&self, ctx: &Context) {
        self.lock().remove(&agent_origin(ctx));
    }
}
