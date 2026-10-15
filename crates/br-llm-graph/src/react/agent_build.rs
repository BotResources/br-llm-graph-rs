//! What the builds of the two agents share: the declarations of their
//! keys and the refusals of their own node ids.

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::{Count, Limit};
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::tool::Tool;
use crate::state::{Kind, Schema, SchemaBuilder};
use crate::value::{Key, NodeId};

/// Declares the configuration key a limit reads, as an int.
pub(crate) fn declare_limit(schema: SchemaBuilder, limit: Option<&Limit>) -> SchemaBuilder {
    match limit {
        Some(Limit::Config(key)) => schema.config_by_name(key.clone(), Kind::Int),
        Some(Limit::Fixed(_)) | None => schema,
    }
}

/// Declares a state key the graph takes from its caller. A key the graph
/// already has is refused.
pub(crate) fn declare_input(schema: &mut Schema, key: &Key, kind: Kind) -> Result<(), GraphError> {
    if schema.state.contains_key(key) {
        return Err(GraphError::KeyCollision { key: key.clone() });
    }
    schema.state.insert(key.clone(), kind);
    Ok(())
}

/// Declares the configuration key a count reads, as an int.
pub(crate) fn declare_count(schema: SchemaBuilder, count: &Count) -> SchemaBuilder {
    match count {
        Count::Config(key) => schema.config_by_name(key.clone(), Kind::Int),
        Count::Fixed(_) => schema,
    }
}

/// Without a split, `tools` is a node of the graph, whether tools are
/// declared or not: the refusal of a round-limit node of that id
/// (`DuplicateNode`), to return where the graph's build would refuse a
/// round-limit node named like its other own nodes, after the rules of the
/// ReAct loop.
pub(crate) fn tools_node_clash(
    tool_nodes: &[(NodeId, Vec<Arc<dyn Tool>>)],
    round_limit: Option<&RoundLimit>,
    tools: &NodeId,
) -> Option<GraphError> {
    let limit_node = round_limit.and_then(|limit| match &limit.on_limit {
        OnLimit::Continue { node, .. } | OnLimit::End { node, .. } => Some(node),
        OnLimit::Error => None,
    });
    match limit_node {
        Some(node) if tool_nodes.is_empty() && node == tools => {
            Some(GraphError::DuplicateNode { id: node.clone() })
        }
        Some(_) | None => None,
    }
}

/// The state key a round limit sets when it is reached, if any.
pub(crate) fn round_limit_flag(round_limit: &RoundLimit) -> Option<Key> {
    match &round_limit.on_limit {
        OnLimit::Error => None,
        OnLimit::Continue { flag, .. } | OnLimit::End { flag, .. } => Some(flag.clone()),
    }
}
