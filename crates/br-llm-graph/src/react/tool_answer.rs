//! How a tool node answers one call it owns: judged by the tool's hook,
//! then run, or answered with an error result.

use br_llm_messages::{Text, ToolCall, ToolResult, ToolResultBlock};

use crate::react::hook::{HookError, Judgment};
use crate::react::tool::{Tool, ToolError, ToolOutput};
use crate::state::{Config, State};

/// How one owned call is answered.
pub(crate) enum Answer {
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

/// Judges `call` by the tool's hook, when there is one, then runs it when it
/// is accepted.
pub(crate) async fn judge_and_run(
    tool: &dyn Tool,
    call: &ToolCall,
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
    Answer::Ran(tool.call(call.arguments.clone(), state, config).await)
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
