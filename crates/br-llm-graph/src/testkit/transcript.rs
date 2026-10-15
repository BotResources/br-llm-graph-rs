//! The private `conversation` of an agent run at top level, rebuilt from
//! what the observer sees applied: no state or checkpoint a run gives back
//! holds it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use br_llm_messages::{Conversation, StreamEvent};

use crate::observe::Observer;
use crate::origin::Origin;
use crate::run::{Cursor, PendingEntry};
use crate::state::{Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

/// An observer that keeps the `conversation` of the run at top level, as the
/// updates applied to it build it, and forwards every event to `inner`.
pub struct Transcript {
    inner: Arc<dyn Observer>,
    kept: Mutex<Option<State>>,
}

impl Transcript {
    pub fn new(inner: Arc<dyn Observer>) -> Self {
        Self {
            inner,
            kept: Mutex::new(None),
        }
    }

    /// The conversation as the last applied update left it, if any was.
    pub fn conversation(&self) -> Option<Conversation> {
        let key = Key::new("conversation").unwrap();
        self.kept
            .lock()
            .unwrap()
            .as_ref()
            .map(|state| state.conversation(&key).unwrap().clone())
    }
}

impl Observer for Transcript {
    fn node_started(&self, origin: &Origin, node: &NodeId) {
        self.inner.node_started(origin, node);
    }

    fn node_finished(&self, origin: &Origin, node: &NodeId) {
        self.inner.node_finished(origin, node);
    }

    fn stream(&self, origin: &Origin, key: &Key, event: &StreamEvent) {
        self.inner.stream(origin, key, event);
    }

    fn applied(&self, origin: &Origin, update: &Update) {
        let key = Key::new("conversation").unwrap();
        if origin.occurrence.is_root() && update.key() == &key {
            let mut kept = self.kept.lock().unwrap();
            let state = kept.get_or_insert_with(|| {
                let schema = Schema::builder()
                    .state_by_name(key.clone(), Kind::Conversation)
                    .build();
                let empty = Value::conversation(Conversation::new());
                State::new(schema, BTreeMap::from([(key.clone(), empty)])).unwrap()
            });
            state.apply_batch(std::slice::from_ref(update)).unwrap();
        }
        self.inner.applied(origin, update);
    }

    fn checkpoint(&self, origin: &Origin, state: &State, cursor: &Cursor) {
        self.inner.checkpoint(origin, state, cursor);
    }

    fn run_finished(&self, origin: &Origin, end: &EndLabel) {
        self.inner.run_finished(origin, end);
    }

    fn recorded(&self, origin: &Origin, entry: &PendingEntry) {
        self.inner.recorded(origin, entry);
    }
}
