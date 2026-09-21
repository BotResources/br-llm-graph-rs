use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::edge::Edge;
use crate::graph::node::Node;
use crate::run::Cursor;
use crate::state::Schema;
use crate::value::NodeId;

pub(crate) struct NodeEntry {
    pub node: Box<dyn Node>,
    pub is_join: bool,
}

pub struct Graph {
    schema: Schema,
    entry: NodeId,
    order: Vec<NodeId>,
    nodes: BTreeMap<NodeId, NodeEntry>,
    edges: BTreeMap<NodeId, Box<dyn Edge>>,
}

impl Graph {
    pub(crate) fn assemble(
        schema: Schema,
        entry: NodeId,
        order: Vec<NodeId>,
        nodes: BTreeMap<NodeId, NodeEntry>,
        edges: BTreeMap<NodeId, Box<dyn Edge>>,
    ) -> Self {
        Self {
            schema,
            entry,
            order,
            nodes,
            edges,
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub fn entry(&self) -> &NodeId {
        &self.entry
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
