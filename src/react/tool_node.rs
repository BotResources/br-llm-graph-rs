use std::collections::HashMap;
use std::sync::Arc;

use br_llm_messages::{Author, Text, ToolCall, ToolName, ToolResult, ToolResultBlock, TurnId};
use futures_util::StreamExt;
use futures_util::stream;

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Limit, Node, NodeFuture};
use crate::react::helpers::{check_enabled, enabled_names, last_turn_by_author, pending_calls};
use crate::react::tool::{Tool, ToolFuture};
use crate::state::{Config, Schema, State};
use crate::update::Update;
use crate::value::Key;

/// Runs the pending calls of the agent's last turn that name one of `tools`.
///
/// With `enabled` (the `enabled` key of the `LlmNode`), a call naming a tool
/// the key does not hold is not executed: it is answered with an error result
/// saying so, and the turn goes on.
///
/// At build, `enabled` must be a list-of-strings state key
/// (`EnabledKeyMismatch`) and the key of `max_concurrency` an int
/// configuration key (`LimitKeyMismatch`).
pub struct ToolNode {
    pub key: Key,
    pub author: Author,
    pub tools: Vec<Arc<dyn Tool>>,
    pub max_concurrency: Option<Limit>,
    pub enabled: Option<Key>,
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
            let enabled = match &self.enabled {
                Some(key) => Some(enabled_names(state, key)?),
                None => None,
            };
            let is_enabled = |name: &ToolName| {
                enabled
                    .as_ref()
                    .is_none_or(|names| names.contains(&name.as_str()))
            };
            // Each owned call, with its tool when it runs, `None` when refused.
            let selected: Vec<(&ToolCall, Option<Arc<dyn Tool>>)> =
                pending_calls(state, &self.key, &self.author)?
                    .into_iter()
                    .filter_map(|call| {
                        owned
                            .get(&call.name)
                            .map(|tool| (call, is_enabled(&call.name).then(|| tool.clone())))
                    })
                    .collect();
            let runs = selected.iter().filter(|(_, tool)| tool.is_some()).count();

            let width = match &self.max_concurrency {
                Some(limit) => limit.resolve(config)?.get(),
                None => runs.max(1),
            };
            let calls: Vec<ToolFuture<'_>> = selected
                .iter()
                .filter_map(|(call, tool)| {
                    tool.as_ref()
                        .map(|tool| tool.call(call.arguments.clone(), state, config))
                })
                .collect();
            let mut outcomes = stream::iter(calls)
                .buffered(width)
                .collect::<Vec<_>>()
                .await
                .into_iter();

            let mut updates = Vec::new();
            for (call, tool) in selected {
                if tool.is_none() {
                    updates.push(refusal(&self.key, &turn_id, call));
                    continue;
                }
                let Some(outcome) = outcomes.next() else {
                    break;
                };
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

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        check_enabled(schema, self.enabled.as_ref())?;
        match &self.max_concurrency {
            Some(limit) => limit.check(schema),
            None => Ok(()),
        }
    }
}

/// The error result of a call naming a tool that is not enabled.
fn refusal(key: &Key, turn: &TurnId, call: &ToolCall) -> Update {
    let content = Text::new(format!(
        "Tool {} is not enabled for this run: this call was not executed.",
        call.name
    ))
    .map(|text| vec![ToolResultBlock::text(text)])
    .unwrap_or_default();
    Update::PushResult {
        key: key.clone(),
        turn: turn.clone(),
        result: ToolResult::new(call.id.clone(), call.name.clone(), content, true),
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
