//! The tools of the examples: each builds its declaration once, when it is
//! made.

use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_clients_core::{Parameter, ParameterKind, Parameters, ToolSpec};
use br_llm_graph::{Config, State, Tool, ToolContext, ToolFuture, ToolOutput};
use br_llm_messages::{Text, ToolName, ToolResultBlock};
use serde_json::Value as Json;

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
    fn call<'a>(
        &'a self,
        _call: &'a ToolContext,
        _arguments: Json,
        _s: &'a State,
        _c: &'a Config,
    ) -> ToolFuture<'a> {
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
    fn call<'a>(
        &'a self,
        _call: &'a ToolContext,
        _arguments: Json,
        _s: &'a State,
        _c: &'a Config,
    ) -> ToolFuture<'a> {
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
    fn call<'a>(
        &'a self,
        _call: &'a ToolContext,
        _arguments: Json,
        _s: &'a State,
        _c: &'a Config,
    ) -> ToolFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("file written").expect("non-empty"),
            )]))
        })
    }
}

/// Starts background work and returns at once: the host delivers its
/// completion later, through the control sender, as a runtime input.
pub struct StartTaskTool {
    spec: ToolSpec,
}

impl StartTaskTool {
    pub fn new() -> Self {
        Self {
            spec: tool_spec(
                "start_task",
                "start background work, return at once",
                Vec::new(),
            ),
        }
    }
}

impl Tool for StartTaskTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }
    fn call<'a>(
        &'a self,
        _call: &'a ToolContext,
        _arguments: Json,
        _s: &'a State,
        _c: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async {
            Ok(ToolOutput::text(vec![ToolResultBlock::text(
                Text::new("task-1 started").expect("non-empty"),
            )]))
        })
    }
}
