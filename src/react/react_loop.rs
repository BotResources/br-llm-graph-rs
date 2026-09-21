use std::collections::HashMap;
use std::sync::Arc;

use br_llm_messages::{ToolName, TurnState};

use crate::error::GraphError;
use crate::graph::{Always, Edge, FnEdge, GraphBuilder, Target};
use crate::react::helpers::last_turn_state;
use crate::react::llm_node::LlmNode;
use crate::react::tool::Tool;
use crate::react::tool_node::ToolNode;
use crate::value::NodeId;

pub struct ReactLoop {
    pub llm: NodeId,
    pub tool_nodes: Vec<(NodeId, Vec<Arc<dyn Tool>>)>,
    pub after: Target,
}

impl ReactLoop {
    pub fn add(self, builder: GraphBuilder, llm_node: LlmNode) -> Result<GraphBuilder, GraphError> {
        self.check_partition(&llm_node)?;

        let key = llm_node.key.clone();
        let author = llm_node.author.clone();
        let mut builder = builder.join(self.llm.clone(), llm_node);
        for (id, tools) in &self.tool_nodes {
            builder = builder.node(
                id.clone(),
                ToolNode {
                    key: key.clone(),
                    author: author.clone(),
                    tools: tools.clone(),
                },
            );
        }

        builder = builder.edge(self.llm.clone(), self.llm_edge(key, author));
        for (id, _) in &self.tool_nodes {
            builder = builder.edge(id.clone(), Always(Target::Node(self.llm.clone())));
        }
        Ok(builder)
    }

    fn check_partition(&self, llm_node: &LlmNode) -> Result<(), GraphError> {
        let mut counts: HashMap<ToolName, usize> = HashMap::new();
        for (_, tools) in &self.tool_nodes {
            for tool in tools {
                *counts.entry(tool.spec().name).or_insert(0) += 1;
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
        let after = self.after.clone();
        FnEdge::new(move |state, _config| {
            let conversation = state.conversation(&key)?;
            match last_turn_state(conversation, &author) {
                Some(TurnState::AwaitingToolResults { .. }) => Ok(tool_targets.clone()),
                _ => Ok(vec![after.clone()]),
            }
        })
    }
}
