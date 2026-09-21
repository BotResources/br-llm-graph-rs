use std::future::Future;
use std::pin::Pin;

use br_llm_messages::{Step, StreamEvent, ToolName, WireMessage};
use serde_json::Value;

pub type ModelError = Box<dyn std::error::Error + Send + Sync>;

pub type ModelFuture<'a> = Pin<Box<dyn Future<Output = Result<Step, ModelError>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq)]
pub enum OutputMode {
    Text,
    Structured { schema: Value },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolSpec {
    pub name: ToolName,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub system: Option<String>,
    pub messages: Vec<WireMessage>,
    pub tools: Vec<ToolSpec>,
    pub output: OutputMode,
}

pub trait StreamSink: Sync {
    fn event(&self, event: &StreamEvent);
}

pub trait Model: Send + Sync {
    fn complete<'a>(&'a self, request: Request, sink: &'a dyn StreamSink) -> ModelFuture<'a>;
}
