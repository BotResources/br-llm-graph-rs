use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::edge::{Edge, Target};
use crate::graph::node::Node;
use crate::graph::signature::Signature;
use crate::run::Cursor;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::{Key, NodeId};

pub(crate) struct NodeEntry {
    pub node: Box<dyn Node>,
    pub is_join: bool,
}

/// The edge of a node, with the targets it declared when the graph was
/// built: the run checks these, and the description writes them, so the two
/// never disagree.
pub(crate) struct EdgeEntry {
    pub edge: Box<dyn Edge>,
    pub declared: Option<Vec<Target>>,
}

impl EdgeEntry {
    pub(crate) fn new(edge: Box<dyn Edge>) -> Self {
        let declared = edge.declared().map(<[Target]>::to_vec);
        Self { edge, declared }
    }
}

/// A built graph: a schema, an entry, nodes and one edge per node, and a
/// signature. It does not change and serves any number of runs.
pub struct Graph {
    schema: Schema,
    /// The private keys of the graph, beside `schema`: see `private.rs`.
    private: BTreeMap<Key, Kind>,
    /// `schema` and the private keys: the schema its nodes check against and
    /// the state of its runs.
    run_schema: Schema,
    signature: Signature,
    entry: NodeId,
    order: Vec<NodeId>,
    nodes: BTreeMap<NodeId, NodeEntry>,
    edges: BTreeMap<NodeId, EdgeEntry>,
}

pub(crate) struct Parts {
    pub schema: Schema,
    pub private: BTreeMap<Key, Kind>,
    pub run_schema: Schema,
    pub signature: Signature,
    pub entry: NodeId,
    pub order: Vec<NodeId>,
    pub nodes: BTreeMap<NodeId, NodeEntry>,
    pub edges: BTreeMap<NodeId, EdgeEntry>,
}

impl Graph {
    pub(crate) fn assemble(parts: Parts) -> Self {
        Self {
            schema: parts.schema,
            private: parts.private,
            run_schema: parts.run_schema,
            signature: parts.signature,
            entry: parts.entry,
            order: parts.order,
            nodes: parts.nodes,
            edges: parts.edges,
        }
    }

    /// The schema of the states and configurations given to and given back
    /// by a run of this graph. A private key of the graph (an agent's
    /// `conversation`) is not part of it.
    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    /// The schema of the state a run of this graph works on: `schema` and
    /// the graph's private keys.
    pub(crate) fn run_schema(&self) -> &Schema {
        &self.run_schema
    }

    /// The private keys of the graph, with their kinds.
    pub(crate) fn private(&self) -> &BTreeMap<Key, Kind> {
        &self.private
    }

    /// The keys it takes as inputs and gives back as outputs.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Its entry node.
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
        self.edges.get(id).map(|entry| entry.edge.as_ref())
    }

    /// The targets the edge of `id` declared when the graph was built;
    /// `None` when they are undeclared.
    pub(crate) fn declared(&self, id: &NodeId) -> Option<&[Target]> {
        self.edges
            .get(id)
            .and_then(|entry| entry.declared.as_deref())
    }

    /// Whether it holds the node `id`.
    pub fn contains(&self, id: &NodeId) -> bool {
        self.nodes.contains_key(id)
    }

    /// A run of this graph takes a state made from its schema
    /// (`StateSchemaMismatch`) and a configuration made from its
    /// configuration keys (`ConfigSchemaMismatch`).
    pub(crate) fn check_schemas(&self, state: &State, config: &Config) -> Result<(), GraphError> {
        if state.schema() != &self.schema {
            return Err(GraphError::StateSchemaMismatch);
        }
        if config.schema() != &self.schema.config {
            return Err(GraphError::ConfigSchemaMismatch);
        }
        Ok(())
    }

    /// Refuses a cursor naming a node the graph does not hold (`UnknownNode`).
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
