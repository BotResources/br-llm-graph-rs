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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCalls {
    Allowed,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub system: Option<String>,
    pub messages: Vec<WireMessage>,
    pub tools: Vec<ToolSpec>,
    pub tool_calls: ToolCalls,
    pub output: OutputMode,
    /// Whether the model thinks natively before replying: `Some(true)` on,
    /// `Some(false)` off, `None` the provider's default.
    pub thinking: Option<bool>,
}

pub trait StreamSink: Sync {
    fn event(&self, event: &StreamEvent);
}

/// A model adapter: turns a `Request` into one reply of a provider.
///
/// Each adapter translates the generic settings of the request for its
/// provider: `tool_calls` to the setting that disables tool calls, `thinking`
/// to the provider's native thinking switch or budget (`None` leaves the
/// provider's default).
///
/// For `OutputMode::Structured`, `complete` returns a step whose structured
/// block is valid against the schema, or an error. Validating the block and
/// retrying belong to the adapter: the graph reads the block as given.
pub trait Model: Send + Sync {
    fn complete<'a>(&'a self, request: Request, sink: &'a dyn StreamSink) -> ModelFuture<'a>;
}
