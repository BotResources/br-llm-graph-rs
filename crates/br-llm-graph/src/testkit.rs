use std::sync::Mutex;

use br_llm_messages::TurnId;

use crate::graph::{Context, IdRequest, IdSource};
use crate::observe::Observer;
use crate::origin::Origin;
use crate::run::Cursor;
use crate::state::State;
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

/// A deterministic id source: the n-th request it has not seen yet gets the
/// n-th id (`t1`, `t2`... for turns; a UUIDv7 ending with `n` for threads),
/// and a request it has seen gets the id it was given then. Shared by the
/// runs of a test, it gives a resumed run the ids it had.
pub struct SeqIds {
    given: Mutex<Vec<IdRequest>>,
}

impl SeqIds {
    pub fn new() -> Self {
        Self {
            given: Mutex::new(Vec::new()),
        }
    }

    /// The sequence number of `request`, from 1: a new one when it is new.
    fn number(&self, request: &IdRequest) -> usize {
        let mut given = self.given.lock().unwrap();
        match given.iter().position(|seen| seen == request) {
            Some(index) => index + 1,
            None => {
                given.push(request.clone());
                given.len()
            }
        }
    }
}

impl IdSource for SeqIds {
    fn thread_id(&self, request: &IdRequest) -> crate::value::ThreadId {
        let number = self.number(request);
        crate::value::ThreadId::new(format!("01900000-0000-7000-8000-{number:012x}")).unwrap()
    }

    fn turn_id(&self, request: &IdRequest) -> TurnId {
        TurnId::new(format!("t{}", self.number(request))).unwrap()
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
    fn node_started(&self, _origin: &Origin, node: &NodeId) {
        self.push(format!("started {node}"));
    }

    fn node_finished(&self, _origin: &Origin, node: &NodeId) {
        self.push(format!("finished {node}"));
    }

    fn applied(&self, _origin: &Origin, update: &Update) {
        self.push(format!("applied {update}"));
    }

    fn checkpoint(&self, _origin: &Origin, _state: &State, cursor: &Cursor) {
        let active: Vec<&str> = cursor.active.iter().map(NodeId::as_str).collect();
        self.push(format!("checkpoint [{}]", active.join(",")));
    }

    fn run_finished(&self, _origin: &Origin, end: &EndLabel) {
        self.push(format!("finished run {end}"));
    }
}

pub fn context(observer: std::sync::Arc<dyn Observer>) -> Context {
    Context::new(observer, std::sync::Arc::new(SeqIds::new()))
}

/// The reason `GraphBuilder::build` refused a node.
pub fn refusal<T>(result: Result<T, crate::error::GraphError>) -> crate::error::GraphError {
    match result {
        Err(crate::error::GraphError::InvalidNode { source, .. }) => *source,
        Err(other) => panic!("expected a refused node, got: {other}"),
        Ok(_) => panic!("expected a refused node, the graph was built"),
    }
}
