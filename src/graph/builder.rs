use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::graph::edge::Edge;
use crate::graph::graph::{Graph, NodeEntry};
use crate::graph::map::Map;
use crate::graph::node::Node;
use crate::state::{Kind, Schema};
use crate::value::{Key, NodeId};

struct MapKeys {
    list: Key,
    item: Key,
    output: Key,
    results: Key,
}

pub struct GraphBuilder {
    schema: Schema,
    entry: Option<NodeId>,
    nodes: Vec<(NodeId, NodeEntry)>,
    edges: Vec<(NodeId, Box<dyn Edge>)>,
    maps: Vec<(NodeId, MapKeys)>,
}

impl GraphBuilder {
    pub fn new(schema: Schema) -> Self {
        Self {
            schema,
            entry: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            maps: Vec::new(),
        }
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

    pub fn map(mut self, id: NodeId, map: Map) -> Self {
        let keys = MapKeys {
            list: map.list.clone(),
            item: map.item.clone(),
            output: map.output.clone(),
            results: map.results.clone(),
        };
        self.maps.push((id.clone(), keys));
        self.nodes.push((
            id,
            NodeEntry {
                node: Box::new(map),
                is_join: false,
            },
        ));
        self
    }

    pub fn edge(mut self, from: NodeId, edge: impl Edge + 'static) -> Self {
        self.edges.push((from, Box::new(edge)));
        self
    }

    pub fn build(self) -> Result<Graph, GraphError> {
        let GraphBuilder {
            schema,
            entry,
            nodes: raw_nodes,
            edges: raw_edges,
            maps,
        } = self;

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
            edges.insert(from, edge);
        }

        for id in &order {
            if !edges.contains_key(id) {
                return Err(GraphError::NodeWithoutEdge { id: id.clone() });
            }
        }

        for (id, keys) in &maps {
            check_map(&schema, id, keys)?;
        }

        Ok(Graph::assemble(schema, entry, order, nodes, edges))
    }
}

fn check_map(schema: &Schema, id: &NodeId, keys: &MapKeys) -> Result<(), GraphError> {
    let element = match schema.state.get(&keys.list) {
        Some(Kind::List { element }) => element.as_ref().clone(),
        Some(_) | None => return Err(GraphError::MapKeyMismatch { node: id.clone() }),
    };
    match schema.state.get(&keys.item) {
        Some(item_kind) if *item_kind == element => {}
        Some(_) | None => return Err(GraphError::MapKeyMismatch { node: id.clone() }),
    }
    let output_kind = match schema.state.get(&keys.output) {
        Some(kind) => kind.clone(),
        None => return Err(GraphError::MapKeyMismatch { node: id.clone() }),
    };
    match schema.state.get(&keys.results) {
        Some(Kind::List { element }) if element.as_ref() == &output_kind => Ok(()),
        Some(_) | None => Err(GraphError::MapKeyMismatch { node: id.clone() }),
    }
}

#[cfg(test)]
#[path = "builder_tests.rs"]
mod tests;
