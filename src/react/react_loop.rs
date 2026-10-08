use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use br_llm_messages::{ToolName, TurnState};

use crate::error::GraphError;
use crate::graph::{Always, Edge, FnEdge, GraphBuilder, Limit, Target};
use crate::react::helpers::{last_turn_state, pending_calls};
use crate::react::llm_node::LlmNode;
use crate::react::round_limit::{LimitedLlm, RoundLimit, completed_rounds};
use crate::react::tool::Tool;
use crate::react::tool_node::ToolNode;
use crate::value::NodeId;

pub struct ReactLoop {
    pub llm: NodeId,
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    pub after: Target,
    pub tool_concurrency: Option<Limit>,
    pub round_limit: Option<RoundLimit>,
}

impl ReactLoop {
    pub fn add(self, builder: GraphBuilder, llm_node: LlmNode) -> Result<GraphBuilder, GraphError> {
        self.check_partition(&llm_node)?;
        if let Some(limit) = &self.tool_concurrency {
            limit.check(builder.schema())?;
        }
        if let Some(round_limit) = &self.round_limit {
            round_limit.check(builder.schema())?;
        }

        let key = llm_node.key.clone();
        let author = llm_node.author.clone();
        let mut builder = match &self.round_limit {
            Some(round_limit) => builder.join(
                self.llm.clone(),
                LimitedLlm {
                    llm: llm_node,
                    max_rounds: round_limit.max_rounds.clone(),
                },
            ),
            None => builder.join(self.llm.clone(), llm_node),
        };
        for (id, tools) in &self.tool_nodes {
            builder = builder.node(
                id.clone(),
                ToolNode {
                    key: key.clone(),
                    author: author.clone(),
                    tools: tools.clone(),
                    max_concurrency: self.tool_concurrency.clone(),
                },
            );
        }

        builder = builder.edge(self.llm.clone(), self.llm_edge(key.clone(), author.clone()));
        for (id, _) in &self.tool_nodes {
            builder = builder.edge(id.clone(), Always(Target::Node(self.llm.clone())));
        }
        if let Some(round_limit) = &self.round_limit {
            builder = round_limit.add_limit_node(builder, key, author, &self.llm, &self.after);
        }
        Ok(builder)
    }

    fn check_partition(&self, llm_node: &LlmNode) -> Result<(), GraphError> {
        let declared: HashSet<ToolName> =
            llm_node.tools.iter().map(|tool| tool.spec().name).collect();
        let mut counts: HashMap<ToolName, usize> = HashMap::new();
        for (_, tools) in &self.tool_nodes {
            for tool in tools {
                let name = tool.spec().name;
                if !declared.contains(&name) {
                    return Err(GraphError::ToolNotDeclared { name });
                }
                *counts.entry(name).or_insert(0) += 1;
            }
        }
        for tool in &llm_node.tools {
            let name = tool.spec().name;
            match counts.get(&name).copied().unwrap_or(0) {
                0 => return Err(GraphError::ToolNotCovered { name }),
                1 => {}
                _ => return Err(GraphError::ToolCoveredTwice { name }),
            }
        }
        Ok(())
    }

    fn llm_edge(
        &self,
        key: crate::value::Key,
        author: br_llm_messages::Author,
    ) -> impl Edge + 'static {
        let tool_targets: Vec<Target> = self
            .tool_nodes
            .iter()
            .map(|(id, _)| Target::Node(id.clone()))
            .collect();
        let covered: HashSet<ToolName> = self
            .tool_nodes
            .iter()
            .flat_map(|(_, tools)| tools.iter().map(|tool| tool.spec().name))
            .collect();
        let after = self.after.clone();
        let llm = self.llm.clone();
        let round_limit = self.round_limit.clone();
        FnEdge::new(move |state, config| {
            let conversation = state.conversation(&key)?;
            match last_turn_state(conversation, &author) {
                Some(TurnState::AwaitingToolResults { .. }) => {
                    if let Some(round_limit) = &round_limit {
                        let done = completed_rounds(conversation, &author);
                        let max = round_limit.max_rounds.resolve(config)?.get();
                        if done >= max {
                            return round_limit.reached(&llm, done, max);
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
    }
}
