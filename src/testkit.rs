use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use br_llm_messages::TurnId;

use crate::graph::{Context, IdSource};
use crate::observe::Observer;
use crate::run::Cursor;
use crate::state::State;
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

pub struct SeqIds {
    next: AtomicU64,
}

impl SeqIds {
    pub fn new() -> Self {
        Self {
            next: AtomicU64::new(1),
        }
    }
}

impl IdSource for SeqIds {
    fn turn_id(&self) -> TurnId {
        let index = self.next.fetch_add(1, Ordering::SeqCst);
        TurnId::new(format!("t{index}")).unwrap()
    }
}

#[derive(Default)]
pub struct Recorder {
    pub log: Mutex<Vec<String>>,
}

impl Recorder {
    pub fn lines(&self) -> Vec<String> {
        self.log.lock().map(|log| log.clone()).unwrap_or_default()
    }

    fn push(&self, line: String) {
        if let Ok(mut log) = self.log.lock() {
            log.push(line);
        }
    }
}

impl Observer for Recorder {
    fn node_started(&self, node: &NodeId) {
        self.push(format!("started {node}"));
    }

    fn node_finished(&self, node: &NodeId) {
        self.push(format!("finished {node}"));
    }

    fn applied(&self, update: &Update) {
        self.push(format!("applied {update}"));
    }

    fn checkpoint(&self, _state: &State, cursor: &Cursor) {
        let active: Vec<&str> = cursor.active.iter().map(NodeId::as_str).collect();
        self.push(format!("checkpoint [{}]", active.join(",")));
    }

    fn run_finished(&self, end: &EndLabel) {
        self.push(format!("finished run {end}"));
    }
}

pub fn context(observer: std::sync::Arc<dyn Observer>) -> Context {
    Context::new(observer, std::sync::Arc::new(SeqIds::new()))
}
