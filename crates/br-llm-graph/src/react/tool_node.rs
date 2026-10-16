use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use br_llm_messages::{Author, Text, ToolCall, ToolName, ToolResult, ToolResultBlock};
use futures_util::future::join_all;

use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Limit, Node, NodeError, NodeFuture};
use crate::react::agent_call::ThreadBinding;
use crate::react::helpers::{check_enabled, enabled_names, last_turn_by_author, pending_calls};
use crate::react::hook::{HookError, Judgment};
use crate::react::round_writer::RoundWriters;
use crate::react::tool::{Tool, ToolError, ToolOutput};
use crate::react::tool_gate::SharedGates;
use crate::react::turn_writer::TurnWriter;
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
/// not executed. An error of a hook fails the node and that call is not
/// executed: no call that has not started (taken its slot) is started
/// afterwards, the calls already started, judging or running, finish, and
/// the node's results are dropped. The error returned is the first, in call
/// order, of the hooks that failed. A failed node records nothing, so a
/// resumed run judges and runs its calls again, with the same call ids: the
/// effects of tools are at-least-once.
///
/// At most `max_concurrency` calls run at once (every call at once when
/// `None`); the results keep the call order. The tool nodes of one ReAct loop
/// share `gates`, so their limit bounds their calls together: at most that
/// many calls at once across all of them.
///
/// For a durable agent, the round's results are written to the agent's open
/// turn, as one tool results item, before the node gives back its updates
/// (see `DurableTools`): a tool's updates are applied after its result is in
/// the thread.
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
    /// For a durable agent: where the round's results are written.
    pub(crate) durable: Option<DurableTools>,
}

/// How the tool nodes of a durable agent write a round: every tool node of
/// the loop deposits its results in the round of its superstep, and the last
/// one pushes the round's item whole into the agent's open turn, before any
/// of them gives back its updates.
#[derive(Clone)]
pub(crate) struct DurableTools {
    pub(crate) thread: ThreadBinding,
    pub(crate) rounds: Arc<RoundWriters>,
    /// The number of tool nodes of the loop.
    pub(crate) nodes: usize,
    /// The 0-based place of this node among them, in registration order.
    pub(crate) node: usize,
}

/// How one owned call is answered.
enum Answer {
    /// The call names a tool the `enabled` key does not hold.
    NotEnabled,
    /// A hook of another call failed before this one started: it was not
    /// started.
    NotStarted,
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
        // Taken first: dropped on any error below, it tells the other tool
        // nodes of the round that it will not be written.
        let seat = self
            .durable
            .as_ref()
            .map(|durable| durable.rounds.seat(state, durable.nodes, durable.node));
        let conversation = state.conversation(&self.key)?;
        let Some(turn) = last_turn_by_author(conversation, &self.author) else {
            if let Some(seat) = seat {
                seat.write(Vec::new(), |_| async { Ok(()) }).await?;
            }
            return Ok(Vec::new());
        };
        let turn_id = turn.id().clone();
        let index = turn.items().len();
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
        // Set by the first hook that fails: no call starts after it.
        let failed = AtomicBool::new(false);
        let failed = &failed;
        let answers = join_all(selected.iter().map(|(call, tool)| async move {
            let Some(tool) = tool else {
                return Answer::NotEnabled;
            };
            // Checked before the slot too, not to wait for a slot only to
            // give it back.
            if failed.load(Ordering::SeqCst) {
                return Answer::NotStarted;
            }
            let _pass = match gate {
                Some(gate) => Some(gate.enter().await),
                None => None,
            };
            if failed.load(Ordering::SeqCst) {
                return Answer::NotStarted;
            }
            let answer = judge_and_run(tool.as_ref(), call, state, config).await;
            if matches!(answer, Answer::HookFailed(_)) {
                failed.store(true, Ordering::SeqCst);
            }
            answer
        }))
        .await;
        drop(lease);

        // Each call's result, in call order, with the tool's updates.
        let mut answered: Vec<(ToolResult, Vec<Update>)> = Vec::new();
        for ((call, _), answer) in selected.iter().zip(answers) {
            match answer {
                Answer::NotEnabled => {
                    let text = format!(
                        "Tool {} is not enabled for this run: this call was not executed.",
                        call.name
                    );
                    answered.push((error_result(call, text_block(&text)), Vec::new()));
                }
                // Only after a hook failed, whose error this loop returns.
                Answer::NotStarted => {}
                Answer::Ran(Ok(output)) => {
                    let result = ToolResult::new(
                        call.id.clone(),
                        call.name.clone(),
                        output.content,
                        output.is_error,
                    );
                    answered.push((result, output.updates));
                }
                Answer::Ran(Err(error)) => answered.push((
                    error_result(call, text_or(&error.to_string(), "tool call failed")),
                    Vec::new(),
                )),
                Answer::Refused(reason) => answered.push((
                    error_result(call, text_or(&reason, "tool call refused")),
                    Vec::new(),
                )),
                Answer::HookFailed(error) => return Err(error),
            }
        }
        if let (Some(seat), Some(durable)) = (seat, &self.durable) {
            let results = answered.iter().map(|(result, _)| result.clone()).collect();
            let writer = TurnWriter {
                store: durable.thread.store.as_ref(),
                thread: durable.thread.thread(state)?,
                turn: turn_id.clone(),
            };
            seat.write(results, |all| async move {
                if all.is_empty() {
                    return Ok(());
                }
                writer.push_results(index, &all).await
            })
            .await?;
        }
        let mut updates = Vec::new();
        for (result, extra) in answered {
            updates.push(Update::PushResult {
                key: self.key.clone(),
                turn: turn_id.clone(),
                result,
            });
            updates.extend(extra);
        }
        Ok(updates)
    }
}

/// Judges `call` by the tool's hook, when there is one, then runs it when it
/// is accepted.
async fn judge_and_run(tool: &dyn Tool, call: &ToolCall, state: &State, config: &Config) -> Answer {
    if let Some(hook) = tool.hook() {
        match hook.judge(&call.id, &call.arguments, state, config).await {
            Ok(Judgment::Accept) => {}
            Ok(Judgment::Refuse { reason }) => return Answer::Refused(reason),
            Err(error) => return Answer::HookFailed(error),
        }
    }
    Answer::Ran(tool.call(call.arguments.clone(), state, config).await)
}

/// One text block holding `text`; none when it is blank.
fn text_block(text: &str) -> Vec<ToolResultBlock> {
    Text::new(text)
        .map(|text| vec![ToolResultBlock::text(text)])
        .unwrap_or_default()
}

/// One text block holding `text`, or `fallback` when `text` is blank.
fn text_or(text: &str, fallback: &str) -> Vec<ToolResultBlock> {
    match Text::new(text) {
        Ok(text) => vec![ToolResultBlock::text(text)],
        Err(_) => text_block(fallback),
    }
}

/// The error result of `call`, holding `content`.
fn error_result(call: &ToolCall, content: Vec<ToolResultBlock>) -> ToolResult {
    ToolResult::new(call.id.clone(), call.name.clone(), content, true)
}
