use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::edge::Edge;
use crate::graph::graph::{Graph, NodeEntry, Parts};
use crate::graph::map::Map;
use crate::graph::node::{CheckSite, Node};
use crate::graph::signature::Signature;
use crate::graph::subgraph::SubGraph;
use crate::state::{Kind, Schema};
use crate::value::{Key, NodeId};

pub struct GraphBuilder {
    schema: Schema,
    private: BTreeMap<Key, Kind>,
    entry: Option<NodeId>,
    nodes: Vec<(NodeId, NodeEntry)>,
    edges: Vec<(NodeId, Box<dyn Edge>)>,
    inputs: Vec<Key>,
    outputs: Vec<Key>,
}

impl GraphBuilder {
    pub fn new(schema: Schema) -> Self {
        Self {
            schema,
            private: BTreeMap::new(),
            entry: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
        }
    }

    /// Declares a state key as an input of the graph: a run started with
    /// `Graph::start_state` must be given its value. It takes a typed key
    /// (`&StateKey<T>`) or a key by name.
    pub fn input(mut self, key: impl Into<Key>) -> Self {
        self.inputs.push(key.into());
        self
    }

    /// Declares a state key as an output of the graph. It takes a typed key
    /// (`&StateKey<T>`) or a key by name.
    pub fn output(mut self, key: impl Into<Key>) -> Self {
        self.outputs.push(key.into());
        self
    }

    pub fn entry(mut self, id: NodeId) -> Self {
        self.entry = Some(id);
        self
    }

    pub fn node(mut self, id: NodeId, node: impl Node + 'static) -> Self {
        self.nodes.push((
            id,
            NodeEntry {
                node: Box::new(node),
                is_join: false,
            },
        ));
        self
    }

    pub fn join(mut self, id: NodeId, node: impl Node + 'static) -> Self {
        self.nodes.push((
            id,
            NodeEntry {
                node: Box::new(node),
                is_join: true,
            },
        ));
        self
    }

    /// Registers a map node, as `node` does.
    pub fn map(self, id: NodeId, map: Map) -> Self {
        self.node(id, map)
    }

    /// Registers a node that calls another graph, as `node` does.
    pub fn subgraph(self, id: NodeId, call: SubGraph) -> Self {
        self.node(id, call)
    }

    pub(crate) fn schema(&self) -> &Schema {
        &self.schema
    }

    /// Declares the private state key `key`, of kind `kind`: a key of the
    /// state the graph's runs work on, never part of a state or a checkpoint
    /// a run takes or gives back (see `private.rs`). A key the schema
    /// already declares is refused at build (`KeyCollision`).
    // Until the agents declare their `conversation` through it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn private(mut self, key: Key, kind: Kind) -> Self {
        self.private.insert(key, kind);
        self
    }

    pub fn edge(mut self, from: NodeId, edge: impl Edge + 'static) -> Self {
        self.edges.push((from, Box::new(edge)));
        self
    }

    pub fn build(self) -> Result<Graph, GraphError> {
        let GraphBuilder {
            schema,
            private,
            entry,
            nodes: raw_nodes,
            edges: raw_edges,
            inputs,
            outputs,
        } = self;

        schema.check_defaults()?;
        let signature = Signature::resolve(&schema, inputs, outputs)?;
        let mut run_schema = schema.clone();
        for (key, kind) in &private {
            if run_schema.state.insert(key.clone(), kind.clone()).is_some() {
                return Err(GraphError::KeyCollision { key: key.clone() });
            }
        }

        let mut order = Vec::with_capacity(raw_nodes.len());
        let mut nodes = BTreeMap::new();
        for (id, entry) in raw_nodes {
            if nodes.contains_key(&id) {
                return Err(GraphError::DuplicateNode { id });
            }
            order.push(id.clone());
            nodes.insert(id, entry);
        }

        let entry = entry.ok_or(GraphError::MissingEntry)?;
        if !nodes.contains_key(&entry) {
            return Err(GraphError::UnknownEntry { id: entry });
        }

        let mut edges = BTreeMap::new();
        for (from, edge) in raw_edges {
            if !nodes.contains_key(&from) {
                return Err(GraphError::EdgeFromUnknownNode { id: from });
            }
            if edges.contains_key(&from) {
                return Err(GraphError::DuplicateEdge { id: from });
            }
            edges.insert(from, edge);
        }

        for id in &order {
            if !edges.contains_key(id) {
                return Err(GraphError::NodeWithoutEdge { id: id.clone() });
            }
        }

        for id in &order {
            if let Some(entry) = nodes.get(id) {
                entry
                    .node
                    .check(&run_schema, CheckSite::Graph)
                    .map_err(|source| GraphError::InvalidNode {
                        node: id.clone(),
                        source: Box::new(source),
                    })?;
            }
        }

        Ok(Graph::assemble(Parts {
            schema,
            private,
            run_schema,
            signature,
            entry,
            order,
            nodes,
            edges,
        }))
    }
}

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
