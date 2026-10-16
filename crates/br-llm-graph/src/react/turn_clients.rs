//! The client of each turn of an agent: asked of its model at the turn's
//! first round, kept for the turn's other rounds, so a turn is run by one
//! client.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_llm_clients_core::Client;
use br_llm_messages::TurnId;

use crate::origin::Origin;

/// The client kept for one call: the turn it runs, and its client.
type Kept = HashMap<Origin, (TurnId, Arc<dyn Client>)>;

/// The client kept for the turn an agent's call is writing, per call: one
/// turn at a time, under the agent's origin (its run id and the occurrence
/// that calls its graph), so concurrent calls of one graph never share one.
///
/// The agent's entry forgets the call's client, so the first round of a
/// call asks the model again, even on a turn it continues (an own turn the
/// opening of an agent in memory ends with, awaiting a step).
#[derive(Default)]
pub(crate) struct TurnClients {
    kept: Mutex<Kept>,
}

impl TurnClients {
    fn lock(&self) -> MutexGuard<'_, Kept> {
        self.kept.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The client kept for the turn `turn` of the call at `origin`.
    pub(crate) fn kept(&self, origin: &Origin, turn: &TurnId) -> Option<Arc<dyn Client>> {
        self.lock()
            .get(origin)
            .filter(|(kept, _)| kept == turn)
            .map(|(_, client)| client.clone())
    }

    /// Keeps `client` for the turn `turn` of the call at `origin`, in place
    /// of the client of its earlier turn.
    pub(crate) fn keep(&self, origin: Origin, turn: TurnId, client: Arc<dyn Client>) {
        self.lock().insert(origin, (turn, client));
    }

    /// Forgets the client of the call at `origin`: its turn has ended, or
    /// the call enters again.
    pub(crate) fn forget(&self, origin: &Origin) {
        self.lock().remove(origin);
    }
}
