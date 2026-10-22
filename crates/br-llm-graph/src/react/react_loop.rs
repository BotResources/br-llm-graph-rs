use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use br_llm_clients_core::BuildError;
use br_llm_messages::{Author, ToolName, TurnState};

use crate::error::GraphError;
use crate::graph::{Always, Edge, FnEdge, GraphBuilder, Target};
use crate::react::agent_call::{AgentCall, ThreadBinding};
use crate::react::given::LoopNode;
use crate::react::helpers::last_turn_state;
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
///
/// The tool nodes are `split`, or without a split one node, `tools_node`,
/// that runs every tool the model call declares, even without a tool: the
/// loop always has a first tool node (`ToolNodes`), which also answers the
/// pending calls that name a tool no tool node runs, so a reply that calls
/// one goes to the tool nodes like any other and the turn goes on.
pub(crate) struct ReactLoop {
    pub(crate) llm: NodeId,
    /// How the tools are split between tool nodes; empty: no split.
    pub(crate) split: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    /// The id of the one tool node without a split.
    pub(crate) tools_node: NodeId,
    pub(crate) after: Target,
    /// The thread of a durable agent, which its tool nodes and its limit
    /// node write to.
    pub(crate) thread: Option<ThreadBinding>,
}

/// One tool node of a loop: its id and the tools it runs.
type ToolNodeSpec = (NodeId, Vec<Arc<dyn Tool>>);

/// The tool nodes of a loop, in registration order: one at least, and the
/// first also answers the calls that name a tool no tool node runs.
struct ToolNodes {
    first: ToolNodeSpec,
    others: Vec<ToolNodeSpec>,
}

impl ToolNodes {
    /// The nodes of `split`; without a split, one node `id` running every
    /// one of `declared`.
    fn new(split: Vec<ToolNodeSpec>, declared: &[Arc<dyn Tool>], id: NodeId) -> Self {
        let mut split = split.into_iter();
        match split.next() {
            Some(first) => Self {
                first,
                others: split.collect(),
            },
            None => Self {
                first: (id, declared.to_vec()),
                others: Vec::new(),
            },
        }
    }

    fn iter(&self) -> impl Iterator<Item = &ToolNodeSpec> {
        std::iter::once(&self.first).chain(&self.others)
    }

    fn len(&self) -> usize {
        1 + self.others.len()
    }
}

impl ReactLoop {
    pub(crate) fn add<N: LoopNode>(
        self,
        builder: GraphBuilder,
        llm_node: LlmNode<N>,
    ) -> Result<GraphBuilder, GraphError> {
        self.check_names(&llm_node)?;
        let tool_nodes = ToolNodes::new(self.split, &llm_node.tools, self.tools_node);
        check_partition(&tool_nodes, &llm_node)?;
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
        let call = llm_node.call.clone();
        // The first tool node answers the calls that name none of them.
        let declared: Vec<ToolName> = llm_node
            .tools
            .iter()
            .map(|tool| tool.spec().name().clone())
            .collect();
        let mut builder = builder.join(self.llm.clone(), llm_node);
        // One gate per superstep for all the tool nodes: the limit bounds
        // their calls together.
        let gates = Arc::new(SharedGates::default());
        // One round of results per superstep for all the tool nodes of a
        // durable agent: its item is written whole.
        let rounds = Arc::new(RoundWriters::default());
        let nodes = tool_nodes.len();
        for (node, (id, tools)) in tool_nodes.iter().enumerate() {
            builder = builder.node(
                id.clone(),
                ToolNode {
                    key: key.clone(),
                    author: author.clone(),
                    tools: tools.clone(),
                    max_concurrency: tool_concurrency.clone(),
                    enabled: enabled.clone(),
                    gates: gates.clone(),
                    declared: (node == 0).then(|| declared.clone()),
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
            llm_edge(
                (&self.llm, &tool_nodes, &self.after),
                (key.clone(), call),
                author.clone(),
                round_limit.clone(),
            ),
        );
        for (id, _) in tool_nodes.iter() {
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
}

/// Each tool of a tool node must be declared (`ToolNotDeclared`), and each
/// declared tool run by exactly one tool node (`ToolNotCovered`,
/// `ToolCoveredTwice`).
fn check_partition<N: LoopNode>(
    tool_nodes: &ToolNodes,
    llm_node: &LlmNode<N>,
) -> Result<(), GraphError> {
    let declared: HashSet<&ToolName> = llm_node
        .tools
        .iter()
        .map(|tool| tool.spec().name())
        .collect();
    let mut counts: HashMap<&ToolName, usize> = HashMap::new();
    for (_, tools) in tool_nodes.iter() {
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

/// The edge after the model call `llm`: to the tool nodes while the reply
/// calls tools, to the round-limit node past the last allowed round, else
/// where the loop ends (`after`). It declares all of them. A reply that
/// calls a tool no tool node runs goes to the tool nodes too: the first
/// answers that call; past the last allowed round, the limit handles it
/// first. The rounds counted are those of the open turn, after those of the
/// turns it continues, which `call` lists failed (see `round_count`).
fn llm_edge(
    (llm, tool_nodes, after): (&NodeId, &ToolNodes, &Target),
    (key, call): (Key, AgentCall),
    author: Author,
    round_limit: Option<RoundLimit>,
) -> impl Edge + 'static {
    let tool_targets: Vec<Target> = tool_nodes
        .iter()
        .map(|(id, _)| Target::Node(id.clone()))
        .collect();
    let (llm, after) = (llm.clone(), after.clone());
    // The tool nodes, the round-limit node when it has one, then where the
    // loop ends.
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
                    let failed = call.failed_turns(state)?;
                    if completed_rounds(conversation, &author, &failed) >= max {
                        return round_limit.reached(&llm, conversation, &author, &failed, max);
                    }
                }
                Ok(tool_targets.clone())
            }
            _ => Ok(vec![after.clone()]),
        }
    })
    .targets(declared)
}
