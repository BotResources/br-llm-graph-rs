//! What the builds of the two agents share: the declarations of their
//! keys and the refusals of their own node ids.

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::GraphBuilder;
use crate::graph::{Count, Limit};
use crate::react::agent_call::{AgentCall, ThreadBinding};
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::thread_store::ThreadStore;
use crate::react::tool::Tool;
use crate::state::{Kind, Schema, SchemaBuilder};
use crate::value::{Key, NodeId};

/// The keys every agent has: its inputs `opening` and, durable, `thread`
/// (also an output), its output `answer`, and its private keys
/// `conversation`, `turn_rank` (where the call stands among its turns, see
/// `Position`) and, for the generator-critic, `call_turns` (the ids of the
/// call's turns).
pub(crate) struct AgentKeys {
    pub(crate) opening: Key,
    pub(crate) answer: Key,
    pub(crate) conversation: Key,
    pub(crate) position: Key,
    /// `call_turns`, a private key of the generator-critic only.
    pub(crate) turns: Key,
    /// Whether the agent keeps the ids of its call's turns in `turns`.
    keeps_turns: bool,
    /// `Some` when the agent is durable.
    pub(crate) thread: Option<Key>,
    /// `thread`, which no flag may name, durable or not.
    pub(crate) thread_name: Key,
}

impl AgentKeys {
    /// The keys of a ReAct agent.
    pub(crate) fn new(durable: bool) -> Result<Self, GraphError> {
        let thread_name = Key::new("thread")?;
        Ok(Self {
            opening: Key::new("opening")?,
            answer: Key::new("answer")?,
            conversation: Key::new("conversation")?,
            position: Key::new("turn_rank")?,
            turns: Key::new("call_turns")?,
            keeps_turns: false,
            thread: durable.then(|| thread_name.clone()),
            thread_name,
        })
    }

    /// The keys of a generator-critic: a ReAct agent's, and `call_turns`.
    pub(crate) fn with_turns(durable: bool) -> Result<Self, GraphError> {
        Ok(Self {
            keeps_turns: true,
            ..Self::new(durable)?
        })
    }

    fn private(&self) -> Vec<(&Key, Kind)> {
        let mut private = vec![
            (&self.conversation, Kind::Conversation),
            (&self.position, Kind::Int),
        ];
        if self.keeps_turns {
            private.push((&self.turns, Kind::list(Kind::Str)));
        }
        private
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

    /// The call of the agent's graph: its `opening`, where it stands, its
    /// thread when it is durable, and, for the generator-critic, the list of
    /// its turns.
    pub(crate) fn call(&self, store: Option<Arc<dyn ThreadStore>>) -> AgentCall {
        AgentCall {
            opening: self.opening.clone(),
            position: self.position.clone(),
            thread: match (&self.thread, store) {
                (Some(key), Some(store)) => Some(ThreadBinding {
                    store,
                    key: key.clone(),
                }),
                _ => None,
            },
            turns: self.keeps_turns.then(|| self.turns.clone()),
        }
    }

    /// Declares the signature of the agent's graph: its inputs `opening`,
    /// `thread` when durable, then `declared`; its outputs `answer`, then
    /// `outputs`, the round limit's `flag` and `thread` when durable.
    pub(crate) fn signature(
        &self,
        mut builder: GraphBuilder,
        declared: Vec<Key>,
        outputs: &[&Key],
        flag: Option<&Key>,
    ) -> GraphBuilder {
        let inputs = std::iter::once(&self.opening)
            .chain(&self.thread)
            .chain(&declared);
        for key in inputs {
            builder = builder.input(key.clone());
        }
        let outputs = std::iter::once(&self.answer)
            .chain(outputs.iter().copied())
            .chain(flag)
            .chain(&self.thread);
        for key in outputs {
            builder = builder.output(key.clone());
        }
        builder
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

/// The split of `tools` between tool nodes: `tool_nodes`, or without a
/// split one node, `id`, that runs them all. Without a tool too, so a reply
/// that calls a tool the agent does not have is answered there, and a
/// round-limit node of that id collides with it (`DuplicateNode`).
pub(crate) fn split_tools(
    tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    tools: &[Arc<dyn Tool>],
    id: &NodeId,
) -> Vec<(NodeId, Vec<Arc<dyn Tool>>)> {
    if tool_nodes.is_empty() {
        vec![(id.clone(), tools.to_vec())]
    } else {
        tool_nodes
    }
}

/// The state key a round limit sets when it is reached, if any.
pub(crate) fn round_limit_flag(round_limit: &RoundLimit) -> Option<Key> {
    match &round_limit.on_limit {
        OnLimit::Error => None,
        OnLimit::Continue { flag, .. } | OnLimit::End { flag, .. } => Some(flag.clone()),
    }
}
