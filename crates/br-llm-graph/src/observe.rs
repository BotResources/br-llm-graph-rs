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
    /// A node of the superstep, after the superstep completes; not when a
    /// cancel drops the nodes while they run.
    fn node_finished(&self, origin: &Origin, node: &NodeId) {
        let _ = (origin, node);
    }
    /// A model stream event, with the conversation key of the calling node,
    /// or the output key of an `LlmCall` whose request is not a conversation
    /// key. A reply asked for again (see `LlmCall::reply_retries`) streams
    /// too: the events of every attempt come one after the other, with no
    /// event between two attempts, a refused one possibly without its
    /// `Finish`, the next one starting again at block 0.
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

/// An observer that ignores every event.
pub struct NoopObserver;

impl Observer for NoopObserver {}
