use std::sync::Arc;

use br_llm_messages::TurnId;

use crate::observe::Observer;
use crate::origin::{OccurrenceKey, Origin, RunId, Segment};
use crate::value::NodeId;

pub trait IdSource: Send + Sync {
    fn turn_id(&self) -> TurnId;
}

/// What the engine hands a node besides state and configuration: the
/// observer, the id source, and where the node runs (run id and occurrence).
#[derive(Clone)]
pub struct Context {
    pub observer: Arc<dyn Observer>,
    pub ids: Arc<dyn IdSource>,
    origin: Origin,
}

impl Context {
    /// A top-level context: empty occurrence, default run id.
    pub fn new(observer: Arc<dyn Observer>, ids: Arc<dyn IdSource>) -> Self {
        Self {
            observer,
            ids,
            origin: Origin::default(),
        }
    }

    pub fn with_run_id(mut self, run: RunId) -> Self {
        self.origin.run = run;
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

    /// This context one segment deeper.
    pub fn child(&self, segment: Segment) -> Context {
        let mut child = self.clone();
        child.origin.occurrence = self.origin.occurrence.child(segment);
        child
    }
}
