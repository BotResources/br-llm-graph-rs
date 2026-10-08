use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::edge::Edge;
use crate::graph::node::Node;
use crate::graph::signature::Signature;
use crate::run::Cursor;
use crate::state::{Schema, State, Value};
use crate::value::{Key, NodeId};

pub(crate) struct NodeEntry {
    pub node: Box<dyn Node>,
    pub is_join: bool,
}

pub struct Graph {
    schema: Schema,
    signature: Signature,
    entry: NodeId,
    order: Vec<NodeId>,
    nodes: BTreeMap<NodeId, NodeEntry>,
    edges: BTreeMap<NodeId, Box<dyn Edge>>,
}

pub(crate) struct Parts {
    pub schema: Schema,
    pub signature: Signature,
    pub entry: NodeId,
    pub order: Vec<NodeId>,
    pub nodes: BTreeMap<NodeId, NodeEntry>,
    pub edges: BTreeMap<NodeId, Box<dyn Edge>>,
}

impl Graph {
    pub(crate) fn assemble(parts: Parts) -> Self {
        Self {
            schema: parts.schema,
            signature: parts.signature,
            entry: parts.entry,
            order: parts.order,
            nodes: parts.nodes,
            edges: parts.edges,
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    pub fn entry(&self) -> &NodeId {
        &self.entry
    }

    /// The state a run of this graph starts from: the declared inputs take the
    /// given values, every other key its default. Only declared inputs may be
    /// given, each once, and every declared input must be.
    pub fn start_state(
        &self,
        inputs: impl IntoIterator<Item = (Key, Value)>,
    ) -> Result<State, GraphError> {
        let mut values: BTreeMap<Key, Value> = BTreeMap::new();
        for (key, value) in inputs {
            if values.contains_key(&key) {
                return Err(GraphError::InputGivenTwice { key });
            }
            let Some(kind) = self.schema.state.get(&key) else {
                return Err(GraphError::UnknownKey { key });
            };
            if !self.signature.inputs.contains_key(&key) {
                return Err(GraphError::NotAnInput { key });
            }
            if !value.matches(kind) {
                return Err(GraphError::KindMismatch {
                    key,
                    expected: kind.clone(),
                    found: value.tag(),
                });
            }
            values.insert(key, value);
        }
        for key in self.signature.inputs.keys() {
            if !values.contains_key(key) {
                return Err(GraphError::MissingInput { key: key.clone() });
            }
        }
        for key in self.schema.state.keys() {
            if values.contains_key(key) {
                continue;
            }
            let value = self
                .schema
                .default_value(key)
                .ok_or_else(|| GraphError::UnknownKey { key: key.clone() })?;
            values.insert(key.clone(), value);
        }
        State::new(self.schema.clone(), values)
    }

    pub(crate) fn order(&self) -> &[NodeId] {
        &self.order
    }

    pub(crate) fn node(&self, id: &NodeId) -> Option<&dyn Node> {
        self.nodes.get(id).map(|entry| entry.node.as_ref())
    }

    pub(crate) fn is_join(&self, id: &NodeId) -> bool {
        self.nodes
            .get(id)
            .map(|entry| entry.is_join)
            .unwrap_or(false)
    }

    pub(crate) fn edge(&self, id: &NodeId) -> Option<&dyn Edge> {
        self.edges.get(id).map(|edge| edge.as_ref())
    }

    pub fn contains(&self, id: &NodeId) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn validate_cursor(&self, cursor: &Cursor) -> Result<(), GraphError> {
        for id in cursor.active.iter().chain(cursor.deferred.iter()) {
            if !self.contains(id) {
                return Err(GraphError::UnknownNode { id: id.clone() });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "start_state_tests.rs"]
mod start_state_tests;
