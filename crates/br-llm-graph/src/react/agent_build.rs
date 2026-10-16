//! What the builds of the two agents share: the declarations of their
//! keys and the refusals of their own node ids.

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::GraphBuilder;
use crate::graph::{Count, Limit};
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::tool::Tool;
use crate::state::{Kind, Schema, SchemaBuilder};
use crate::value::{Key, NodeId};

/// The keys every agent has: its inputs `opening` and, durable, `thread`
/// (also an output), its output `answer`, and its private keys
/// `conversation` and `turn_rank` (where the call stands among its turns,
/// see `Position`).
pub(crate) struct AgentKeys {
    pub(crate) opening: Key,
    pub(crate) answer: Key,
    pub(crate) conversation: Key,
    pub(crate) position: Key,
    /// `Some` when the agent is durable.
    pub(crate) thread: Option<Key>,
    /// `thread`, which no flag may name, durable or not.
    thread_name: Key,
}

impl AgentKeys {
    pub(crate) fn new(durable: bool) -> Result<Self, GraphError> {
        let thread_name = Key::new("thread")?;
        Ok(Self {
            opening: Key::new("opening")?,
            answer: Key::new("answer")?,
            conversation: Key::new("conversation")?,
            position: Key::new("turn_rank")?,
            thread: durable.then(|| thread_name.clone()),
            thread_name,
        })
    }

    fn private(&self) -> [(&Key, Kind); 2] {
        [
            (&self.conversation, Kind::Conversation),
            (&self.position, Kind::Int),
        ]
    }

    /// The state keys the graph keeps for itself, which nothing given may
    /// name: `opening`, `answer`, the private keys, `thread` when durable and
    /// the round limit's `flag`, if any.
    pub(crate) fn reserved(&self, flag: Option<&Key>) -> Vec<Key> {
        let mut reserved = vec![self.opening.clone(), self.answer.clone()];
        reserved.extend(self.private().into_iter().map(|(key, _)| key.clone()));
        reserved.extend(self.thread.clone());
        reserved.extend(flag.cloned());
        reserved
    }

    /// Declares the private keys in `schema` while the other keys are
    /// declared, so a key that takes their name collides with them.
    pub(crate) fn reserve(&self, schema: &mut Schema) {
        for (key, kind) in self.private() {
            schema.state.insert(key.clone(), kind);
        }
    }

    /// Takes the private keys out of `schema` again, to declare them as
    /// private keys of the graph.
    pub(crate) fn release(&self, schema: &mut Schema) {
        for (key, _) in self.private() {
            schema.state.remove(key);
        }
    }

    /// Declares the private keys of the agent's graph.
    pub(crate) fn declare_private(&self, builder: GraphBuilder) -> GraphBuilder {
        self.private()
            .into_iter()
            .fold(builder, |builder, (key, kind)| {
                builder.private(key.clone(), kind)
            })
    }

    /// Whether a round limit's flag names a key no flag may name: `answer`,
    /// `opening`, `thread` or `conversation`.
    pub(crate) fn names_flag(&self, flag: &Key) -> bool {
        [
            &self.answer,
            &self.opening,
            &self.thread_name,
            &self.conversation,
        ]
        .contains(&flag)
    }
}

/// Declares the configuration key a limit reads, as an int.
pub(crate) fn declare_limit(schema: SchemaBuilder, limit: Option<&Limit>) -> SchemaBuilder {
    match limit {
        Some(Limit::Config(key)) => schema.config_by_name(key.clone(), Kind::Int),
        Some(Limit::Fixed(_)) | None => schema,
    }
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
