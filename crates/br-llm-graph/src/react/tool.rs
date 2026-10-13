use std::future::Future;
use std::pin::Pin;

use br_llm_messages::ToolResultBlock;
use serde_json::Value;

use crate::react::model::ToolSpec;
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

pub trait Tool: Send + Sync {
    fn spec(&self) -> ToolSpec;
    fn safe(&self) -> bool;
    fn call<'a>(&'a self, arguments: Value, state: &'a State, config: &'a Config)
    -> ToolFuture<'a>;
}
