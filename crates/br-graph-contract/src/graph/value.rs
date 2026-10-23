use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use super::structure::Structure;
use crate::{Edge, Fields, Node, Target};

pub(super) struct EdgeEntry<F: Fields> {
    pub(super) edge: Arc<dyn Edge<F>>,
    pub(super) declared: Vec<Target>,
    positions: HashMap<Target, usize>,
}

impl<F: Fields> EdgeEntry<F> {
    pub(super) fn new(edge: Arc<dyn Edge<F>>, declared: Vec<Target>) -> Self {
        let positions = declared
            .iter()
            .enumerate()
            .map(|(position, target)| (target.clone(), position))
            .collect();
        Self {
            edge,
            declared,
            positions,
        }
    }

    pub(super) fn position(&self, target: &Target) -> Option<usize> {
        self.positions.get(target).copied()
    }
}

pub struct Graph<F: Fields> {
    structure: Structure,
    nodes: HashMap<String, Arc<dyn Node<F>>>,
    edges: HashMap<String, EdgeEntry<F>>,
}

impl<F: Fields> Graph<F> {
    pub(super) fn assemble(
        structure: Structure,
        nodes: HashMap<String, Arc<dyn Node<F>>>,
        edges: HashMap<String, EdgeEntry<F>>,
    ) -> Self {
        Self {
            structure,
            nodes,
            edges,
        }
    }

    pub fn version(&self) -> &str {
        self.structure.version()
    }

    pub fn entries(&self) -> &[String] {
        self.structure.entries()
    }

    pub fn node(&self, name: &str) -> Option<&Arc<dyn Node<F>>> {
        self.nodes.get(name)
    }

    pub fn edge_of(&self, node: &str) -> Option<&Arc<dyn Edge<F>>> {
        self.edges.get(node).map(|entry| &entry.edge)
    }

    pub fn structure(&self) -> Structure {
        self.structure.clone()
    }

    pub(super) fn entry_of(&self, node: &str) -> Option<&EdgeEntry<F>> {
        self.edges.get(node)
    }
}

impl<F: Fields> fmt::Debug for Graph<F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Graph")
            .field("version", &self.version())
            .field("entries", &self.entries())
            .field("nodes", &self.nodes.len())
            .finish()
    }
}
