use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use br_llm_clients_core::BuildError;
use br_llm_messages::{Author, ToolName, TurnState};

use crate::error::GraphError;
use crate::graph::{Always, Edge, FnEdge, GraphBuilder, Target};
use crate::react::agent_call::ThreadBinding;
use crate::react::given::LoopNode;
use crate::react::helpers::{last_turn_state, pending_calls};
use crate::react::llm_node::LlmNode;
use crate::react::round_limit::{RoundLimit, completed_rounds};
use crate::react::round_writer::RoundWriters;
use crate::react::tool::Tool;
use crate::react::tool_gate::SharedGates;
use crate::react::tool_node::{DurableTools, ToolNode};
use crate::value::{Key, NodeId};

/// The ReAct loop, added to a graph under construction: the model call
/// `llm`, the tool nodes, the routing edge after the model call and, with a
/// round limit that does not fail the run, its limit node. The building block
/// of `ReactAgent` and of the generator's generations; not public. The tool
/// concurrency limit and the round limit are the model call's
/// (`LlmNode::tool_concurrency`, `LlmNode::round_limit`), their one source:
/// the tool nodes share that limit, at most that many calls at once across
/// all of them.
pub(crate) struct ReactLoop {
    pub(crate) llm: NodeId,
    pub(crate) tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    pub(crate) after: Target,
    /// The thread of a durable agent, which its tool nodes and its limit
    /// node write to.
    pub(crate) thread: Option<ThreadBinding>,
}

impl ReactLoop {
    pub(crate) fn add<N: LoopNode>(
        self,
        builder: GraphBuilder,
        llm_node: LlmNode<N>,
    ) -> Result<GraphBuilder, GraphError> {
        self.check_names(&llm_node)?;
        self.check_partition(&llm_node)?;
        let tool_concurrency = llm_node.tool_concurrency.clone();
        let round_limit = llm_node.round_limit.clone();
        if let Some(limit) = &tool_concurrency {
            limit.check(builder.schema())?;
        }
        if let Some(round_limit) = &round_limit {
            round_limit.check(builder.schema())?;
        }

        let key = llm_node.key.clone();
        let author = llm_node.author.clone();
        let enabled = llm_node.enabled.clone();
        let mut builder = builder.join(self.llm.clone(), llm_node);
        // One gate per superstep for all the tool nodes: the limit bounds
        // their calls together.
        let gates = Arc::new(SharedGates::default());
        // One round of results per superstep for all the tool nodes of a
        // durable agent: its item is written whole.
        let rounds = Arc::new(RoundWriters::default());
        let nodes = self.tool_nodes.len();
        for (node, (id, tools)) in self.tool_nodes.iter().enumerate() {
            builder = builder.node(
                id.clone(),
                ToolNode {
                    key: key.clone(),
                    author: author.clone(),
                    tools: tools.clone(),
                    max_concurrency: tool_concurrency.clone(),
                    enabled: enabled.clone(),
                    gates: gates.clone(),
                    durable: self.thread.clone().map(|thread| DurableTools {
                        thread,
                        rounds: rounds.clone(),
                        nodes,
                        node,
                    }),
                },
            );
        }

        builder = builder.edge(
            self.llm.clone(),
            self.llm_edge(key.clone(), author.clone(), round_limit.clone()),
        );
        for (id, _) in &self.tool_nodes {
            builder = builder.edge(id.clone(), Always(Target::Node(self.llm.clone())));
        }
        if let Some(round_limit) = &round_limit {
            builder = round_limit.add_limit_node(
                builder,
                key,
                author,
                (&self.llm, &self.after),
                self.thread.clone(),
            );
        }
        Ok(builder)
    }

    /// Two declared tools with one name make a request no client accepts:
    /// the model-call step is refused (`InvalidNode`, its reason `Request`
    /// carrying `DuplicateToolName`), before the rules on tool nodes, which
    /// go by name.
    fn check_names<N: LoopNode>(&self, llm_node: &LlmNode<N>) -> Result<(), GraphError> {
        let mut seen: HashSet<&ToolName> = HashSet::new();
        for tool in &llm_node.tools {
            let name = tool.spec().name();
            if !seen.insert(name) {
                return Err(GraphError::InvalidNode {
                    node: self.llm.clone(),
                    source: Box::new(GraphError::Request(BuildError::DuplicateToolName {
                        name: name.clone(),
                    })),
                });
            }
        }
        Ok(())
    }

    fn check_partition<N: LoopNode>(&self, llm_node: &LlmNode<N>) -> Result<(), GraphError> {
        let declared: HashSet<&ToolName> = llm_node
            .tools
            .iter()
            .map(|tool| tool.spec().name())
            .collect();
        let mut counts: HashMap<&ToolName, usize> = HashMap::new();
        for (_, tools) in &self.tool_nodes {
            for tool in tools {
                let name = tool.spec().name();
                if !declared.contains(name) {
                    return Err(GraphError::ToolNotDeclared { name: name.clone() });
                }
                *counts.entry(name).or_insert(0) += 1;
            }
        }
        for tool in &llm_node.tools {
            let name = tool.spec().name();
            match counts.get(name).copied().unwrap_or(0) {
                0 => return Err(GraphError::ToolNotCovered { name: name.clone() }),
                1 => {}
                _ => return Err(GraphError::ToolCoveredTwice { name: name.clone() }),
            }
        }
        Ok(())
    }

    /// The edge after the model call: to the tool nodes while the reply
    /// calls tools, to the round-limit node past the last allowed round,
    /// else where the loop ends (`after`). It declares all of them.
    fn llm_edge(
        &self,
        key: Key,
        author: Author,
        round_limit: Option<RoundLimit>,
    ) -> impl Edge + 'static {
        let tool_targets: Vec<Target> = self
            .tool_nodes
            .iter()
            .map(|(id, _)| Target::Node(id.clone()))
            .collect();
        let covered: HashSet<ToolName> = self
            .tool_nodes
            .iter()
            .flat_map(|(_, tools)| tools.iter().map(|tool| tool.spec().name().clone()))
            .collect();
        let after = self.after.clone();
        let llm = self.llm.clone();
        // The tool nodes, the round-limit node when it has one, then where
        // the loop ends.
        let limit_node = round_limit
            .as_ref()
            .and_then(RoundLimit::node)
            .map(|node| Target::Node(node.clone()));
        let declared: Vec<Target> = tool_targets
            .iter()
            .cloned()
            .chain(limit_node)
            .chain(std::iter::once(after.clone()))
            .collect();
        FnEdge::new(move |state, config| {
            let conversation = state.conversation(&key)?;
            match last_turn_state(conversation, &author) {
                Some(TurnState::AwaitingToolResults { .. }) => {
                    if let Some(round_limit) = &round_limit {
                        let max = round_limit.max_rounds.resolve(config)?.get();
                        if completed_rounds(conversation, &author) >= max {
                            return round_limit.reached(&llm, conversation, &author, max);
                        }
                    }
                    for call in pending_calls(state, &key, &author)? {
                        if !covered.contains(&call.name) {
                            return Err(GraphError::PendingToolUnsatisfiable {
                                name: call.name.clone(),
                            });
                        }
                    }
                    Ok(tool_targets.clone())
                }
                _ => Ok(vec![after.clone()]),
            }
        })
        .targets(declared)
    }
}
