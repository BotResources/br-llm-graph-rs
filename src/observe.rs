use br_llm_messages::StreamEvent;

use crate::origin::Origin;
use crate::run::Cursor;
use crate::state::State;
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

/// Receives what a run does. Every event carries its `Origin`: the run id and
/// the occurrence it comes from. Events of the run loop carry the occurrence
/// of the run (empty at top level, the calling node's occurrence in a nested
/// run); events a node emits carry the node's own occurrence.
pub trait Observer: Send + Sync {
    fn node_started(&self, origin: &Origin, node: &NodeId) {
        let _ = (origin, node);
    }
    fn node_finished(&self, origin: &Origin, node: &NodeId) {
        let _ = (origin, node);
    }
    fn stream(&self, origin: &Origin, key: &Key, event: &StreamEvent) {
        let _ = (origin, key, event);
    }
    fn applied(&self, origin: &Origin, update: &Update) {
        let _ = (origin, update);
    }
    fn checkpoint(&self, origin: &Origin, state: &State, cursor: &Cursor) {
        let _ = (origin, state, cursor);
    }
    fn run_finished(&self, origin: &Origin, end: &EndLabel) {
        let _ = (origin, end);
    }
    /// A finished occurrence recorded its updates while its superstep is
    /// still open. A host may persist these as they come and merge them into
    /// the pending writes of a checkpoint it keeps.
    fn recorded(&self, origin: &Origin, updates: &[Update]) {
        let _ = (origin, updates);
    }
}

pub struct NoopObserver;

impl Observer for NoopObserver {}
