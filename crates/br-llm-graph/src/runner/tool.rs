//! A runner call as a tool of an agent: the model calls another runner, and
//! reads its answer as the call's result.

use std::sync::Arc;

use br_llm_clients_core::ToolSpec;
use br_llm_messages::{Text, ToolResultBlock};
use serde_json::Value;

use crate::react::{Tool, ToolContext, ToolError, ToolFuture, ToolOutput, text_or};
use crate::runner::answer::{AnswerBlock, RunnerAnswer};
use crate::runner::port::{CallKey, RunnerPort, RunnerReply, RunnerRequest};
use crate::runner::target::RunnerTarget;
use crate::state::{Config, State};

/// A tool that calls another runner through the runner port: its spec is
/// the runner's, its arguments those of the tool call, and its result the
/// child's answer.
///
/// Each call's key is the origin of the tool node that runs it and the id of
/// the tool call (see `CallKey`), so a call run again under its id finds the
/// call it had started. The arguments of the tool call go to the port as
/// they are, a JSON object: the graph does not check them against the
/// spec's parameters, which is the port's to do; arguments that are not an
/// object are a tool error, and the port is not called.
///
/// The result holds the answer's blocks in order, a structured value as its
/// JSON text. An expected failure of the child gives an error result holding
/// the reason (`runner call failed` when it is blank), and the turn goes on.
/// An error of the port is a tool error: an error result holding its
/// message. The tool returns no update.
///
/// Runner tools called in one reply run at once, within the agent's tool
/// concurrency limit, as any tools do; a cancel of the run drops the calls
/// under way, whose children the port cancels. It has no hook: a host that
/// judges its calls gives a tool of its own that holds it, gives its spec
/// and its call, and gives a hook.
pub struct RunnerTool {
    /// The declaration the model reads, given by the runner.
    pub spec: ToolSpec,
    /// The runner type of the runner it calls.
    pub target: RunnerTarget,
    /// The port the calls go through, the host's.
    pub port: Arc<dyn RunnerPort>,
}

impl Tool for RunnerTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call<'a>(
        &'a self,
        call: &'a ToolContext,
        arguments: Value,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            let Value::Object(arguments) = arguments else {
                return Err(ToolError::from(
                    "the arguments of a runner call must be a JSON object",
                ));
            };
            let request = RunnerRequest::new(
                CallKey::new(call.origin.clone(), Some(call.id.clone())),
                self.target.clone(),
                arguments,
            );
            match self.port.call(&request).await? {
                RunnerReply::Answered(answer) => Ok(ToolOutput::text(result_blocks(&answer)?)),
                RunnerReply::Failed { reason } => Ok(ToolOutput {
                    content: text_or(&reason, "runner call failed"),
                    is_error: true,
                    updates: Vec::new(),
                }),
            }
        })
    }
}

/// The blocks of `answer` as a tool result, in order: a text as it is, a
/// structured value as its JSON text.
fn result_blocks(answer: &RunnerAnswer) -> Result<Vec<ToolResultBlock>, ToolError> {
    answer
        .blocks
        .iter()
        .map(|block| {
            Ok(ToolResultBlock::text(match block {
                AnswerBlock::Text(text) => text.clone(),
                // A JSON text is never blank.
                AnswerBlock::Structured(value) => Text::new(value.to_string())?,
            }))
        })
        .collect()
}
