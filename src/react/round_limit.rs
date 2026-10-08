use br_llm_messages::{
    AssistantBlock, Author, Conversation, Step, StopReason, Text, ToolResult, ToolResultBlock,
    TurnItem, TurnState,
};

use crate::error::GraphError;
use crate::graph::{Always, Context, GraphBuilder, Limit, Node, NodeFuture, Target};
use crate::react::helpers::{last_turn_by_author, pending_calls};
use crate::react::llm_node::LlmNode;
use crate::react::model::ToolCalls;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::update::Update;
use crate::value::{Key, NodeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundLimit {
    pub max_rounds: Limit,
    pub on_limit: OnLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnLimit {
    Error,
    Continue { node: NodeId, flag: Key },
    End { node: NodeId, flag: Key },
}

impl RoundLimit {
    pub(crate) fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        self.max_rounds.check(schema)?;
        match &self.on_limit {
            OnLimit::Error => Ok(()),
            OnLimit::Continue { flag, .. } | OnLimit::End { flag, .. } => {
                match schema.state.get(flag) {
                    Some(Kind::Bool) => Ok(()),
                    Some(_) | None => Err(GraphError::FlagKeyMismatch { key: flag.clone() }),
                }
            }
        }
    }

    pub(crate) fn reached(
        &self,
        llm: &NodeId,
        done: usize,
        max: usize,
    ) -> Result<Vec<Target>, GraphError> {
        match &self.on_limit {
            OnLimit::Continue { node, .. } | OnLimit::End { node, .. } if done == max => {
                Ok(vec![Target::Node(node.clone())])
            }
            OnLimit::Error | OnLimit::Continue { .. } | OnLimit::End { .. } => {
                Err(GraphError::ToolLimitReached {
                    node: llm.clone(),
                    max_rounds: max,
                })
            }
        }
    }

    pub(crate) fn add_limit_node(
        &self,
        builder: GraphBuilder,
        key: Key,
        author: Author,
        llm: &NodeId,
        after: &Target,
    ) -> GraphBuilder {
        let (node, flag, end, next) = match &self.on_limit {
            OnLimit::Error => return builder,
            OnLimit::Continue { node, flag } => (node, flag, false, Target::Node(llm.clone())),
            OnLimit::End { node, flag } => (node, flag, true, after.clone()),
        };
        builder
            .node(
                node.clone(),
                LimitNode {
                    key,
                    author,
                    max_rounds: self.max_rounds.clone(),
                    flag: flag.clone(),
                    end,
                },
            )
            .edge(node.clone(), Always(next))
    }
}

pub(crate) fn completed_rounds(conversation: &Conversation, author: &Author) -> usize {
    match last_turn_by_author(conversation, author) {
        Some(turn) if !matches!(turn.state(), TurnState::Finished { .. }) => turn
            .items()
            .iter()
            .filter(|item| matches!(item, TurnItem::ToolResults(_)))
            .count(),
        Some(_) | None => 0,
    }
}

pub(crate) struct LimitedLlm {
    pub llm: LlmNode,
    pub max_rounds: Limit,
}

impl Node for LimitedLlm {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let done = completed_rounds(state.conversation(&self.llm.key)?, &self.llm.author);
            let max = self.max_rounds.resolve(config)?.get();
            let tool_calls = if done > max {
                ToolCalls::Forbidden
            } else {
                ToolCalls::Allowed
            };
            self.llm.step(state, config, ctx, tool_calls).await
        })
    }
}

struct LimitNode {
    key: Key,
    author: Author,
    max_rounds: Limit,
    flag: Key,
    end: bool,
}

impl Node for LimitNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let max = self.max_rounds.resolve(config)?.get();
            let conversation = state.conversation(&self.key)?;
            let Some(turn) = last_turn_by_author(conversation, &self.author) else {
                return Ok(Vec::new());
            };
            let refusal = Text::new(format!(
                "Tool round limit reached ({max}): this call was not executed. Reply without calling tools."
            ))?;
            let mut updates = Vec::new();
            for call in pending_calls(state, &self.key, &self.author)? {
                updates.push(Update::PushResult {
                    key: self.key.clone(),
                    turn: turn.id().clone(),
                    result: ToolResult::new(
                        call.id.clone(),
                        call.name.clone(),
                        vec![ToolResultBlock::text(refusal.clone())],
                        true,
                    ),
                });
            }
            updates.push(Update::Set {
                key: self.flag.clone(),
                value: Value::bool(true),
            });
            if self.end {
                let text = Text::new(format!("Tool round limit reached ({max}): no final reply."))?;
                let step = Step::new(
                    vec![AssistantBlock::Text { text }],
                    StopReason::Other {
                        reason: "tool_round_limit".to_owned(),
                    },
                    None,
                    None,
                )?;
                updates.push(Update::PushStep {
                    key: self.key.clone(),
                    turn: turn.id().clone(),
                    step,
                });
            }
            Ok(updates)
        })
    }
}
