//! The names of the generator-critic graph's own keys and nodes.

use crate::error::GraphError;
use crate::value::{Key, NodeId};

/// The names of the graph's own keys and nodes, beside an agent's.
pub(crate) struct Names {
    pub(crate) validated: Key,
    pub(crate) generations: Key,
    pub(crate) critic_stream: Key,
    pub(crate) prepare: NodeId,
    pub(crate) route: NodeId,
    pub(crate) generate: NodeId,
    pub(crate) check: NodeId,
    pub(crate) critique: NodeId,
    pub(crate) finish: NodeId,
    pub(crate) tools: NodeId,
}

impl Names {
    pub(crate) fn new() -> Result<Self, GraphError> {
        Ok(Self {
            validated: Key::new("is_validated")?,
            generations: Key::new("nb_generations")?,
            critic_stream: Key::new("critic")?,
            prepare: NodeId::new("prepare")?,
            route: NodeId::new("route")?,
            generate: NodeId::new("generate")?,
            check: NodeId::new("check")?,
            critique: NodeId::new("critique")?,
            finish: NodeId::new("finish")?,
            tools: NodeId::new("tools")?,
        })
    }
}
