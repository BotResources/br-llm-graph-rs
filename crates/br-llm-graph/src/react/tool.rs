use std::future::Future;
use std::pin::Pin;

use br_llm_clients_core::ToolSpec;
use br_llm_messages::ToolResultBlock;
use serde_json::Value;

use crate::react::hook::Hook;
use crate::state::{Config, State};
use crate::update::Update;

pub type ToolError = Box<dyn std::error::Error + Send + Sync>;

pub type ToolFuture<'a> = Pin<Box<dyn Future<Output = Result<ToolOutput, ToolError>> + Send + 'a>>;

pub struct ToolOutput {
    pub content: Vec<ToolResultBlock>,
    pub is_error: bool,
    pub updates: Vec<Update>,
}

impl ToolOutput {
    pub fn text(blocks: Vec<ToolResultBlock>) -> Self {
        Self {
            content: blocks,
            is_error: false,
            updates: Vec::new(),
        }
    }

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

    fn call<'a>(&'a self, arguments: Value, state: &'a State, config: &'a Config)
    -> ToolFuture<'a>;
}
