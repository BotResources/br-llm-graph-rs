//! A tool the host implements, and what its call gives back.

use std::future::Future;
use std::pin::Pin;

use br_llm_clients_core::ToolSpec;
use br_llm_messages::ToolResultBlock;
use serde_json::Value;

use crate::react::hook::Hook;
use crate::state::{Config, State};
use crate::update::Update;

/// The error of a tool, of the host's type: its call is answered with an error
/// result holding its message (`tool call failed` when the message is blank),
/// and the turn goes on.
pub type ToolError = Box<dyn std::error::Error + Send + Sync>;

/// One call of a tool under way.
pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>>;

/// What one call of a tool gives back: its result blocks, whether the result
/// is an error, and updates of the state, applied after the result (for a
/// durable agent, after the result is written to the thread).
pub struct ToolOutput {
    /// The blocks of the tool result.
    pub content: Vec<ToolResultBlock>,
    /// Whether the result is an error result.
    pub is_error: bool,
    /// Updates of the state, for example of the enabled key, which changes the
    /// calls that follow.
    pub updates: Vec<Update>,
}

impl ToolOutput {
    /// A result of `blocks`, not an error, without updates.
    pub fn text(blocks: Vec<ToolResultBlock>) -> Self {
        Self {
            content: blocks,
            is_error: false,
            updates: Vec::new(),
        }
    }

    /// This output with the updates `updates`.
    pub fn with_updates(mut self, updates: Vec<Update>) -> Self {
        self.updates = updates;
        self
    }
}

/// A tool the host implements: its declaration, an optional hook, and its
/// effect.
///
/// The ReAct loop runs the pending calls of a reply that name the tool: each
/// call is judged by the hook when there is one, then run with its JSON
/// arguments, the state and the configuration. Running the effect and
/// validating the arguments against the parameters belong to the tool.
pub trait Tool: Send + Sync {
    /// The declaration a model reads to decide to call the tool.
    fn spec(&self) -> &ToolSpec;

    /// The hook that judges each call of the tool before it runs. `None`,
    /// the default: every call is run as is.
    fn hook(&self) -> Option<&dyn Hook> {
        None
    }

    /// Runs one call with its JSON arguments, the state and the configuration
    /// of the run.
    fn call<'a>(&'a self, arguments: Value, state: &'a State, config: &'a Config)
    -> ToolFuture<'a>;
}
