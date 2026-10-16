//! The builder of a graph.

use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::edge::{Edge, Target};
use crate::graph::graph::{EdgeEntry, Graph, NodeEntry, Parts};
use crate::graph::map::Map;
use crate::graph::node::{CheckSite, Node};
use crate::graph::signature::Signature;
use crate::graph::subgraph::SubGraph;
use crate::state::{Kind, Schema};
use crate::value::{Key, NodeId};

/// Builds a graph from a schema, an entry node, nodes (plain, join, map,
/// call), one edge per node, and input and output declarations. `build` checks
/// them in a fixed order (see `build`).
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
    /// A builder over `schema`, with no node.
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

    /// Sets the entry node, where a run without a cursor starts.
    pub fn entry(mut self, id: NodeId) -> Self {
        self.entry = Some(id);
        self
    }

    /// Registers a plain node: targeted by a superstep, it runs in the next
    /// one.
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

    /// Registers a join node: targeted, it is deferred while any plain node is
    /// left to run; when only join nodes are left, they all run in the next
    /// superstep.
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
    pub(crate) fn private(mut self, key: Key, kind: Kind) -> Self {
        self.private.insert(key, kind);
        self
    }

    /// Sets the edge of the node `from`. Every node has exactly one.
    pub fn edge(mut self, from: NodeId, edge: impl Edge + 'static) -> Self {
        self.edges.push((from, Box::new(edge)));
        self
    }

    /// The graph, or the first refusal, in this order: a default on an
    /// undeclared key (`UnknownKey`) or of the wrong kind (`KindMismatch`); an
    /// input or output that is not a state key (`UnknownKey`), a key declared
    /// twice as an input (`DuplicateInput`) or as an output
    /// (`DuplicateOutput`); two nodes with one id (`DuplicateNode`); no entry
    /// (`MissingEntry`), an entry that is not a node (`UnknownEntry`); an edge
    /// from an unknown node (`EdgeFromUnknownNode`), two edges from one node
    /// (`DuplicateEdge`); a node without an edge (`NodeWithoutEdge`); a node
    /// whose check fails, nodes taken in registration order (`InvalidNode`,
    /// the reason as its source); then, after the nodes, a node target an
    /// edge declares that is not a node of the graph (`EdgeToUnknownNode`,
    /// edges in node registration order, targets in declared order). An
    /// edge's undeclared targets, and the end labels it declares, are not
    /// checked.
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
            edges.insert(from, EdgeEntry::new(edge));
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

        for id in &order {
            let declared = edges.get(id).and_then(|entry| entry.declared.as_deref());
            for target in declared.unwrap_or_default() {
                if let Target::Node(target) = target
                    && !nodes.contains_key(target)
                {
                    return Err(GraphError::EdgeToUnknownNode {
                        node: id.clone(),
                        target: target.clone(),
                    });
                }
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

#[cfg(test)]
#[path = "edge_to_unknown_tests.rs"]
mod edge_to_unknown_tests;
