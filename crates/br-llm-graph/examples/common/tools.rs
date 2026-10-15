//! The tools of the examples: each builds its declaration once, when it is
//! made.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_llm_clients_core::{Parameter, ParameterKind, Parameters, ToolSpec};
use br_llm_graph::{
    Config, ControlSender, Key, State, Tool, ToolFuture, ToolOutput, Update, Value,
};
use br_llm_messages::{Text, ToolName, ToolResultBlock};
use serde_json::Value as Json;

use super::runtime_input;

/// The declaration of a tool: built once, when the tool is made.
pub fn tool_spec(name: &str, description: &str, parameters: Vec<Parameter>) -> ToolSpec {
    ToolSpec::new(
        ToolName::new(name).expect("name"),
        description,
        Parameters::new(parameters).expect("parameters"),
    )
    .expect("spec")
}

/// A required string parameter.
pub fn text_parameter(name: &str, description: &str) -> Parameter {
    Parameter::new(name, description, true, ParameterKind::String).expect("parameter")
}

pub struct EchoTool {
    spec: ToolSpec,
}

impl EchoTool {
    pub fn new() -> Self {
        Self {
            spec: tool_spec("echo", "echo the message back", Vec::new()),
        }
    }
}

impl Tool for EchoTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("echoed").expect("non-empty"),
            )]))
        })
    }
}

pub struct SearchTool {
    spec: ToolSpec,
}

impl SearchTool {
    pub fn new() -> Self {
        Self {
            spec: tool_spec(
                "search",
                "search a knowledge base",
                vec![text_parameter("q", "what to search for")],
            ),
        }
    }
}

impl Tool for SearchTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("one result found").expect("non-empty"),
            )]))
        })
    }
}

/// Writes a file: counts its calls, so an example shows which ran.
pub struct WriteFileTool {
    spec: ToolSpec,
    pub calls: AtomicUsize,
}

impl WriteFileTool {
    pub fn new() -> Self {
        Self {
            spec: tool_spec(
                "write_file",
                "write a file",
                vec![
                    text_parameter("path", "where to write"),
                    text_parameter("content", "what to write"),
                ],
            ),
            calls: AtomicUsize::new(0),
        }
    }
}

impl Tool for WriteFileTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("file written").expect("non-empty"),
            )]))
        })
    }
}

pub struct StartTaskTool {
    spec: ToolSpec,
    pub sender: Arc<Mutex<Option<ControlSender>>>,
    pub chat: Key,
    pub pending: Key,
}

impl StartTaskTool {
    pub fn new(sender: Arc<Mutex<Option<ControlSender>>>, chat: Key, pending: Key) -> Self {
        Self {
            spec: tool_spec(
                "start_task",
                "start background work, return at once",
                Vec::new(),
            ),
            sender,
            chat,
            pending,
        }
    }
}

impl Tool for StartTaskTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(&'a self, _arguments: Json, _s: &'a State, _c: &'a Config) -> ToolFuture<'a> {
        let chat = self.chat.clone();
        let handle = self.sender.lock().ok().and_then(|guard| guard.clone());
        if let Some(sender) = handle {
            tokio::spawn(async move {
                tokio::task::yield_now().await;
                sender.send(chat, runtime_input("task_done", "task-1 completed"));
            });
        }
        let pending = self.pending.clone();
        Box::pin(async move {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("task started").expect("non-empty"),
            )])
            .with_updates(vec![Update::Append {
                key: pending,
                value: Value::str("task-1"),
            }]))
        })
    }
}
