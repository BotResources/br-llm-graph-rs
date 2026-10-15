use std::collections::HashMap;
use std::sync::Arc;

use br_llm_messages::{Author, Text, ToolCall, ToolName, ToolResult, ToolResultBlock, TurnId};
use futures_util::future::join_all;

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Limit, Node, NodeError, NodeFuture};
use crate::react::helpers::{check_enabled, enabled_names, last_turn_by_author, pending_calls};
use crate::react::hook::{HookError, Judgment};
use crate::react::tool::{Tool, ToolError, ToolOutput};
use crate::react::tool_gate::SharedGates;
use crate::state::{Config, Schema, State};
use crate::update::Update;
use crate::value::Key;

/// The tool execution of the ReAct loop: runs the pending calls of the
/// agent's last turn that name one of `tools`. Calls naming another node's
/// tools are left to that node; without a turn of the agent it returns
/// nothing.
///
/// Each run call gives a tool result, in call order, followed by the tool's
/// updates; a tool error gives an error result holding its message.
///
/// With `enabled` (the `enabled` key of the model-call step), a call naming a
/// tool the key does not hold is not executed: it is answered with an error
/// result saying so, and the turn goes on.
///
/// A call is judged by its tool's hook, when there is one, after the
/// `enabled` check, inside its concurrency slot, before its tool runs. A
/// refused call is answered with an error result holding the reason and is
/// not executed; an error of a hook fails the node, once every call has
/// ended, with the first such error in call order.
///
/// At most `max_concurrency` calls run at once (every call at once when
/// `None`); the results keep the call order. The tool nodes of one ReAct loop
/// share `gates`, so their limit bounds their calls together: at most that
/// many calls at once across all of them.
///
/// At build, `enabled` must be a list-of-strings state key
/// (`EnabledKeyMismatch`) and the key of `max_concurrency` an int
/// configuration key (`LimitKeyMismatch`).
pub(crate) struct ToolNode {
    pub(crate) key: Key,
    pub(crate) author: Author,
    pub(crate) tools: Vec<Arc<dyn Tool>>,
    pub(crate) max_concurrency: Option<Limit>,
    pub(crate) enabled: Option<Key>,
    pub(crate) gates: Arc<SharedGates>,
}

/// How one enabled call is answered.
enum Answer {
    /// The tool ran and gave this outcome.
    Ran(Result<ToolOutput, ToolError>),
    /// The hook refused the call, with this reason.
    Refused(String),
    /// The hook failed: the node fails.
    HookFailed(HookError),
}

impl Node for ToolNode {
    fn run<'a>(
        &'a self,
        state: &'a State,
        config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(self.run_calls(state, config))
    }

    fn check(&self, schema: &Schema, _site: CheckSite) -> Result<(), GraphError> {
        check_enabled(schema, self.enabled.as_ref())?;
        match &self.max_concurrency {
            Some(limit) => limit.check(schema),
            None => Ok(()),
        }
    }
}

impl ToolNode {
    async fn run_calls(&self, state: &State, config: &Config) -> Result<Vec<Update>, NodeError> {
        let conversation = state.conversation(&self.key)?;
        let Some(turn_id) =
            last_turn_by_author(conversation, &self.author).map(|turn| turn.id().clone())
        else {
            return Ok(Vec::new());
        };
        let owned: HashMap<&ToolName, &Arc<dyn Tool>> = self
            .tools
            .iter()
            .map(|tool| (tool.spec().name(), tool))
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
        // Each owned call, with its tool when it is enabled, `None` when not.
        let selected: Vec<(&ToolCall, Option<&Arc<dyn Tool>>)> =
            pending_calls(state, &self.key, &self.author)?
                .into_iter()
                .filter_map(|call| {
                    owned
                        .get(&call.name)
                        .map(|tool| (call, is_enabled(&call.name).then_some(*tool)))
                })
                .collect();

        let lease = match &self.max_concurrency {
            Some(limit) => Some(self.gates.lease(state, limit.resolve(config)?.get())),
            None => None,
        };
        let gate = lease.as_ref().map(|lease| lease.gate());
        let calls = selected.iter().filter_map(|(call, tool)| {
            tool.map(|tool| async move {
                let _pass = match gate {
                    Some(gate) => Some(gate.enter().await),
                    None => None,
                };
                answer(tool.as_ref(), call, state, config).await
            })
        });
        let mut answers = join_all(calls).await.into_iter();
        drop(lease);

        let mut updates = Vec::new();
        for (call, tool) in selected {
            if tool.is_none() {
                let text = format!(
                    "Tool {} is not enabled for this run: this call was not executed.",
                    call.name
                );
                updates.push(error_result(&self.key, &turn_id, call, &text, ""));
                continue;
            }
            let Some(answer) = answers.next() else {
                break;
            };
            match answer {
                Answer::Ran(Ok(output)) => {
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
                Answer::Ran(Err(error)) => updates.push(error_result(
                    &self.key,
                    &turn_id,
                    call,
                    &error.to_string(),
                    "tool call failed",
                )),
                Answer::Refused(reason) => updates.push(error_result(
                    &self.key,
                    &turn_id,
                    call,
                    &reason,
                    "tool call refused",
                )),
                Answer::HookFailed(error) => return Err(error),
            }
        }
        Ok(updates)
    }
}

/// Judges `call` by the tool's hook, when there is one, then runs it when it
/// is accepted.
async fn answer(tool: &dyn Tool, call: &ToolCall, state: &State, config: &Config) -> Answer {
    if let Some(hook) = tool.hook() {
        match hook.judge(&call.id, &call.arguments, state, config).await {
            Ok(Judgment::Accept) => {}
            Ok(Judgment::Refuse { reason }) => return Answer::Refused(reason),
            Err(error) => return Answer::HookFailed(error),
        }
    }
    Answer::Ran(tool.call(call.arguments.clone(), state, config).await)
}

/// The error result of `call`, holding `text`, or `fallback` when `text` is
/// blank.
fn error_result(key: &Key, turn: &TurnId, call: &ToolCall, text: &str, fallback: &str) -> Update {
    let content = match Text::new(text).or_else(|_| Text::new(fallback)) {
        Ok(text) => vec![ToolResultBlock::text(text)],
        Err(_) => Vec::new(),
    };
    Update::PushResult {
        key: key.clone(),
        turn: turn.clone(),
        result: ToolResult::new(call.id.clone(), call.name.clone(), content, true),
    }
}
