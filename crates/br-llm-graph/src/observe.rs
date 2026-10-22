//! The observer: what a run reports to the host while it runs. Every event is
//! optional for the observer.

use br_llm_messages::StreamEvent;

use crate::origin::Origin;
use crate::run::{Cursor, PendingEntry};
use crate::state::State;
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

/// Receives what a run does. Every event carries its `Origin`: the run id and
/// the occurrence it comes from. Events of the run loop carry the occurrence
/// of the run (empty at top level, the calling node's occurrence in a nested
/// run); events a node emits carry the node's own occurrence.
pub trait Observer: Send + Sync {
    /// A node of the superstep, before the superstep runs.
    fn node_started(&self, origin: &Origin, node: &NodeId) {
        let _ = (origin, node);
    }
    /// A node of the superstep, after the superstep completes, with how it
    /// ended; not when a cancel drops the nodes while they run. A cancel
    /// received just as the superstep completes comes after the event.
    fn node_finished(&self, origin: &Origin, node: &NodeId, outcome: NodeOutcome) {
        let _ = (origin, node, outcome);
    }
    /// A model stream event, with the conversation key of the calling node,
    /// or the output key of an `LlmCall` whose request is not a conversation
    /// key.
    fn stream(&self, origin: &Origin, key: &Key, event: &StreamEvent) {
        let _ = (origin, key, event);
    }
    /// Each update of an applied superstep, in apply order, and each held
    /// input folded into a cancel or failure checkpoint.
    fn applied(&self, origin: &Origin, update: &Update) {
        let _ = (origin, update);
    }
    /// The state and the next cursor after each applied superstep (an empty
    /// cursor once the run ends). For a graph with private keys, such as an
    /// agent's graph and its `conversation`, the state omits them and the
    /// cursor names the graph's entry, as its checkpoints do.
    fn checkpoint(&self, origin: &Origin, state: &State, cursor: &Cursor) {
        let _ = (origin, state, cursor);
    }
    /// The run finished, with its end label.
    fn run_finished(&self, origin: &Origin, end: &EndLabel) {
        let _ = (origin, end);
    }
    /// A finished occurrence recorded its entry while its superstep is still
    /// open: a map item, or a node that has siblings in its superstep (a node
    /// alone in its superstep is recorded too, without this event). A host may
    /// persist these as they come and insert them, under `origin.occurrence`,
    /// into the pending writes of a checkpoint it keeps.
    fn recorded(&self, origin: &Origin, entry: &PendingEntry) {
        let _ = (origin, entry);
    }
}

/// How a node of a superstep ended (`Observer::node_finished`): what the node
/// itself did. A call whose child fails, even by a panic, returns an error
/// (`SubGraphFailed`), or its capture updates under `OnFailure::Capture`; a
/// panic in a map's body is a panic of the map.
///
/// Exhaustive on purpose, as `TurnEnd`: an observer matches it to report each
/// outcome, so an outcome added in a later version must break its build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeOutcome {
    /// The node returned its updates, or reused those it had recorded and did
    /// not run again. Applying the superstep may still refuse them
    /// (`NodeFault::Refused`).
    Updates,
    /// The node returned an error (`NodeFault::Returned`).
    Error,
    /// The node panicked (`NodeFault::Panic`).
    Panic,
}

/// An observer that ignores every event.
pub struct NoopObserver;

impl Observer for NoopObserver {}
