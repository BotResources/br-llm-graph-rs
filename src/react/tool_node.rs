use std::collections::HashMap;
use std::sync::Arc;

use br_llm_messages::{Author, Text, ToolName, ToolResult, ToolResultBlock, TurnId};
use futures_util::StreamExt;
use futures_util::stream;

use crate::graph::{Context, Limit, Node, NodeFuture};
use crate::react::helpers::{last_turn_by_author, pending_calls};
use crate::react::tool::{Tool, ToolFuture};
use crate::state::{Config, State};
use crate::update::Update;
use crate::value::Key;

pub struct ToolNode {
    pub key: Key,
    pub author: Author,
    pub tools: Vec<Arc<dyn Tool>>,
    pub max_concurrency: Option<Limit>,
}

impl Node for ToolNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let conversation = state.conversation(&self.key)?;
            let Some(turn_id) =
                last_turn_by_author(conversation, &self.author).map(|turn| turn.id().clone())
            else {
                return Ok(Vec::new());
            };
            let owned: HashMap<ToolName, Arc<dyn Tool>> = self
                .tools
                .iter()
                .map(|tool| (tool.spec().name, tool.clone()))
                .collect();
            let selected: Vec<(_, Arc<dyn Tool>)> = pending_calls(state, &self.key, &self.author)?
                .into_iter()
                .filter_map(|call| owned.get(&call.name).map(|tool| (call, tool.clone())))
                .collect();

            let width = match &self.max_concurrency {
                Some(limit) => limit.resolve(config)?.get(),
                None => selected.len().max(1),
            };
            let calls: Vec<ToolFuture<'_>> = selected
                .iter()
                .map(|(call, tool)| tool.call(call.arguments.clone(), state, config))
                .collect();
            let outcomes: Vec<_> = stream::iter(calls).buffered(width).collect().await;

            let mut updates = Vec::new();
            for ((call, _tool), outcome) in selected.into_iter().zip(outcomes) {
                match outcome {
                    Ok(output) => {
                        let result = ToolResult::new(
                            call.id.clone(),
                            call.name.clone(),
                            output.content,
                            output.is_error,
                        );
                        updates.push(Update::PushResult {
                            key: self.key.clone(),
                            turn: turn_id.clone(),
                            result,
                        });
                        updates.extend(output.updates);
                    }
                    Err(error) => {
                        updates.push(error_result(
                            &self.key, &turn_id, &call.id, &call.name, error,
                        ));
                    }
                }
            }
            Ok(updates)
        })
    }
}

fn error_result(
    key: &Key,
    turn: &TurnId,
    call_id: &br_llm_messages::ToolCallId,
    name: &ToolName,
    error: crate::react::tool::ToolError,
) -> Update {
    let content = match Text::new(error.to_string()).or_else(|_| Text::new("tool call failed")) {
        Ok(text) => vec![ToolResultBlock::text(text)],
        Err(_) => Vec::new(),
    };
    let result = ToolResult::new(call_id.clone(), name.clone(), content, true);
    Update::PushResult {
        key: key.clone(),
        turn: turn.clone(),
        result,
    }
}
