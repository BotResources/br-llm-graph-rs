//! How a tool node answers one call: judged by the tool's hook, then run,
//! or answered with an error result (a tool the `enabled` key does not hold,
//! a tool no tool node runs, a refusal).

use br_llm_messages::{Text, ToolCall, ToolName, ToolResult, ToolResultBlock};

use crate::origin::Origin;
use crate::react::hook::{HookError, Judgment};
use crate::react::tool::{Tool, ToolContext, ToolError, ToolOutput};
use crate::state::{Config, State};

/// How one call of a tool node is answered.
pub(crate) enum Answer {
    /// The call names a tool the `enabled` key does not hold.
    NotEnabled,
    /// The call names a tool no tool node of the loop runs.
    Unavailable,
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

/// Judges `call` by the tool's hook, when there is one, then runs it when it
/// is accepted, as a call of the tool node at `origin`.
pub(crate) async fn judge_and_run(
    tool: &dyn Tool,
    call: &ToolCall,
    origin: &Origin,
    state: &State,
    config: &Config,
) -> Answer {
    if let Some(hook) = tool.hook() {
        match hook.judge(&call.id, &call.arguments, state, config).await {
            Ok(Judgment::Accept) => {}
            Ok(Judgment::Refuse { reason }) => return Answer::Refused(reason),
            Err(error) => return Answer::HookFailed(error),
        }
    }
    let context = ToolContext::new(call.id.clone(), call.name.clone(), origin.clone());
    Answer::Ran(
        tool.call(&context, call.arguments.clone(), state, config)
            .await,
    )
}

/// One text block holding `text`; none when it is blank.
pub(crate) fn text_block(text: &str) -> Vec<ToolResultBlock> {
    Text::new(text)
        .map(|text| vec![ToolResultBlock::text(text)])
        .unwrap_or_default()
}

/// One text block holding `text`, or `fallback` when `text` is blank.
pub(crate) fn text_or(text: &str, fallback: &str) -> Vec<ToolResultBlock> {
    match Text::new(text) {
        Ok(text) => vec![ToolResultBlock::text(text)],
        Err(_) => text_block(fallback),
    }
}

/// The error result of `call`, holding `content`.
pub(crate) fn error_result(call: &ToolCall, content: Vec<ToolResultBlock>) -> ToolResult {
    ToolResult::new(call.id.clone(), call.name.clone(), content, true)
}

/// The text answering a call of `name`, a tool of the node the `enabled` key
/// does not hold.
pub(crate) fn not_enabled(name: &ToolName) -> String {
    format!("Tool {name} is not enabled for this run: this call was not executed.")
}

/// The text answering a call of `name`, a tool no tool node of the loop
/// runs: it names the tools `offered` to the call, in the order declared, or
/// says that none is.
pub(crate) fn unavailable(name: &ToolName, offered: &[&ToolName]) -> String {
    let not_executed = format!("Tool {name} is not available: this call was not executed.");
    if offered.is_empty() {
        return format!("{not_executed} No tool is available.");
    }
    let offered: Vec<&str> = offered.iter().map(|name| name.as_str()).collect();
    format!(
        "{not_executed} The tools available are: {}.",
        offered.join(", ")
    )
}
