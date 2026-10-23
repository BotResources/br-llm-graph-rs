use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use super::BuildError;
use super::shape::Shape;
use super::structure::{Structure, StructureEdge, StructureNode};
use super::value::{EdgeEntry, Graph};
use crate::{Edge, Fields, Node};

struct NodeSlot<F: Fields> {
    meta: StructureNode,
    node: Arc<dyn Node<F>>,
}

struct EdgeSlot<F: Fields> {
    meta: StructureEdge,
    edge: Arc<dyn Edge<F>>,
}

#[must_use = "a dropped builder loses its nodes, edges and entries"]
pub struct GraphBuilder<F: Fields> {
    version: String,
    nodes: Vec<NodeSlot<F>>,
    edges: Vec<EdgeSlot<F>>,
    entries: Vec<String>,
}

impl<F: Fields> GraphBuilder<F> {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            entries: Vec::new(),
        }
    }

    pub fn node(mut self, node: Arc<dyn Node<F>>) -> Self {
        let meta = StructureNode {
            name: node.name().to_owned(),
            kind: node.kind().to_owned(),
            subkind: node.subkind().to_owned(),
        };
        self.nodes.push(NodeSlot { meta, node });
        self
    }

    pub fn edge(mut self, edge: Arc<dyn Edge<F>>) -> Self {
        let meta = StructureEdge {
            name: edge.name().to_owned(),
            source: edge.source().to_owned(),
            targets: edge.targets().to_vec(),
        };
        self.edges.push(EdgeSlot { meta, edge });
        self
    }

    pub fn entry(mut self, name: impl Into<String>) -> Self {
        self.entries.push(name.into());
        self
    }

    pub fn build(self) -> Result<Graph<F>, BuildError> {
        let shape = Shape {
            version: self.version,
            entries: self.entries,
            nodes: self.nodes.iter().map(|slot| slot.meta.clone()).collect(),
            edges: self.edges.iter().map(|slot| slot.meta.clone()).collect(),
        };
        let structure = Structure::from_shape(shape)?;
        let nodes: HashMap<String, Arc<dyn Node<F>>> = self
            .nodes
            .into_iter()
            .map(|slot| (slot.meta.name, slot.node))
            .collect();
        let edges: HashMap<String, EdgeEntry<F>> = self
            .edges
            .into_iter()
            .map(|slot| {
                let entry = EdgeEntry::new(slot.edge, slot.meta.targets);
                (slot.meta.source, entry)
            })
            .collect();
        Ok(Graph::assemble(structure, nodes, edges))
    }
}

impl<F: Fields> fmt::Debug for GraphBuilder<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphBuilder")
            .field("version", &self.version)
            .field("nodes", &self.nodes.len())
            .field("edges", &self.edges.len())
            .field("entries", &self.entries)
            .finish()
    }
}
