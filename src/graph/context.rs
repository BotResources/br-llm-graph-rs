use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_llm_messages::TurnId;

use crate::observe::Observer;
use crate::origin::{OccurrenceKey, Origin, RunId, Segment};
use crate::run::{PendingEntry, PendingWrites};
use crate::state::Value;
use crate::update::Update;
use crate::value::NodeId;

pub trait IdSource: Send + Sync {
    fn turn_id(&self) -> TurnId;
}

/// What the engine hands a node besides state and configuration: the
/// observer, the id source, where the node runs (run id and occurrence), and
/// the pending-writes recorder of the run.
///
/// The recorder holds an entry per finished occurrence whose superstep is
/// still open: the run loop records every node that returns updates, a map
/// every item (with the item as witness). It is shared by every context
/// derived from this one; `run` gives each run its own recorder, seeded with
/// the pending writes this context holds (empty unless set with
/// `with_pending`).
///
/// Pending writes belong to the run being resumed. A called graph restarts
/// whole: inside it recording is off, for every context derived from it, so
/// `record` and `record_item` do nothing and `recorded` and `recorded_item`
/// find nothing.
#[derive(Clone)]
pub struct Context {
    pub observer: Arc<dyn Observer>,
    pub ids: Arc<dyn IdSource>,
    origin: Origin,
    pending: Arc<Mutex<PendingWrites>>,
    recording: bool,
}

impl Context {
    /// A top-level context: empty occurrence, default run id, no pending
    /// writes.
    pub fn new(observer: Arc<dyn Observer>, ids: Arc<dyn IdSource>) -> Self {
        Self {
            observer,
            ids,
            origin: Origin::default(),
            pending: Arc::default(),
            recording: true,
        }
    }

    pub fn with_run_id(mut self, run: RunId) -> Self {
        self.origin.run = run;
        self
    }

    /// This context with a recorder of its own, holding `pending`: the way to
    /// hand the pending writes of a checkpoint to `run` when resuming it.
    pub fn with_pending(mut self, pending: PendingWrites) -> Self {
        self.pending = Arc::new(Mutex::new(pending));
        self
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn run_id(&self) -> &RunId {
        &self.origin.run
    }

    pub fn occurrence(&self) -> &OccurrenceKey {
        &self.origin.occurrence
    }

    /// The context of node `node` run under this context's occurrence.
    pub fn for_node(&self, node: &NodeId) -> Context {
        self.child(Segment::node(node.clone()))
    }

    pub(crate) fn with_occurrence(&self, occurrence: OccurrenceKey) -> Context {
        let mut context = self.clone();
        context.origin.occurrence = occurrence;
        context
    }

    /// This context one segment deeper.
    pub fn child(&self, segment: Segment) -> Context {
        let mut child = self.clone();
        child.origin.occurrence = self.origin.occurrence.child(segment);
        child
    }

    /// Records the updates of this finished occurrence, so that a run resumed
    /// from a checkpoint taken before its superstep completes does not run it
    /// again. The observer sees the record (`Observer::recorded`).
    pub fn record(&self, updates: &[Update]) {
        self.keep(PendingEntry::new(updates.to_vec()));
    }

    /// Records the updates of this finished map item, which ran on `item`.
    pub fn record_item(&self, item: &Value, updates: &[Update]) {
        self.keep(PendingEntry::witnessed(item.clone(), updates.to_vec()));
    }

    /// What an earlier attempt recorded for this occurrence, recorded with
    /// `record`.
    pub fn recorded(&self) -> Option<Vec<Update>> {
        if !self.recording {
            return None;
        }
        self.store()
            .matching(&self.origin.occurrence, None)
            .map(<[Update]>::to_vec)
    }

    /// What an earlier attempt recorded for this map item, only when it ran on
    /// the same `item`: an entry for another item at this index is ignored.
    pub fn recorded_item(&self, item: &Value) -> Option<Vec<Update>> {
        if !self.recording {
            return None;
        }
        self.store()
            .matching(&self.origin.occurrence, Some(item))
            .map(<[Update]>::to_vec)
    }

    fn keep(&self, entry: PendingEntry) {
        if !self.recording {
            return;
        }
        self.observer.recorded(&self.origin, &entry);
        self.store().insert(self.origin.occurrence.clone(), entry);
    }

    /// The context a called graph runs in: the entries strictly below this
    /// occurrence are removed, and recording is off from here down.
    pub(crate) fn restart_whole(&self) -> Context {
        self.store().drop_below(&self.origin.occurrence);
        let mut scope = self.clone();
        scope.recording = false;
        scope
    }

    /// A copy of every pending write the recorder holds.
    pub fn pending(&self) -> PendingWrites {
        self.store().clone()
    }

    /// The pending writes at or below this context's occurrence.
    pub(crate) fn pending_here(&self) -> PendingWrites {
        self.store().under(&self.origin.occurrence)
    }

    /// Drops the pending writes of `nodes`, run under this context, once
    /// their superstep has completed.
    pub(crate) fn drop_pending(&self, nodes: &[NodeId]) {
        self.store().drop_nodes(&self.origin.occurrence, nodes);
    }

    /// Forgets the pending entry of node `node` run under this context.
    pub(crate) fn forget_node(&self, node: &NodeId) {
        let occurrence = self.origin.occurrence.child(Segment::node(node.clone()));
        self.store().remove(&occurrence);
    }

    /// This context with a recorder of its own seeded with a copy of what
    /// this one holds, recording on: a run records into its own recorder.
    pub(crate) fn isolated(&self) -> Context {
        let mut context = self.clone().with_pending(self.pending());
        context.recording = true;
        context
    }

    fn store(&self) -> MutexGuard<'_, PendingWrites> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
