use br_llm_messages::StreamEvent;

use crate::run::Cursor;
use crate::state::State;
use crate::update::Update;
use crate::value::{EndLabel, Key, NodeId};

pub trait Observer: Send + Sync {
    fn node_started(&self, node: &NodeId) {
        let _ = node;
    }
    fn node_finished(&self, node: &NodeId) {
        let _ = node;
    }
    fn stream(&self, key: &Key, event: &StreamEvent) {
        let _ = (key, event);
    }
    fn applied(&self, update: &Update) {
        let _ = update;
    }
    fn checkpoint(&self, state: &State, cursor: &Cursor) {
        let _ = (state, cursor);
    }
    fn run_finished(&self, end: &EndLabel) {
        let _ = end;
    }
}

pub struct NoopObserver;

impl Observer for NoopObserver {}
