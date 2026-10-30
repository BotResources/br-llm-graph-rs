use std::collections::HashSet;
use std::sync::Arc;

use br_graph_contract::{CancelView, Cause};
use br_llm_clients_core::ToolSpec;
use br_llm_messages::{
    BlockId, Error, ImageBlock, ImageSource, OpaqueJson, PlainBlock, PlainText, RuntimeToolResult,
    StructuredBlock, TextBlock, ToolName,
};

use crate::error::PortError;
use crate::future::PortFuture;
use crate::stream::ResultStream;

const BLANK_REFUSAL: &str = "the request was refused";

pub trait ToolCode: Send + Sync {
    fn call<'a>(
        &'a self,
        arguments: &'a OpaqueJson,
        cancel: CancelView,
        result: &'a mut dyn ResultStream,
    ) -> PortFuture<'a, Result<ToolContent, ToolFault>>;
}

#[derive(Debug, thiserror::Error)]
#[error("a tool faulted")]
pub struct ToolFault {
    #[source]
    cause: Cause,
}

impl ToolFault {
    pub fn new(cause: impl Into<Cause>) -> Self {
        Self {
            cause: cause.into(),
        }
    }

    pub fn cause(&self) -> &Cause {
        &self.cause
    }

    pub fn into_cause(self) -> Cause {
        self.cause
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolBlock {
    Text(PlainText),
    Image(ImageSource),
    Structured(OpaqueJson),
}

impl ToolBlock {
    pub fn text(text: impl Into<String>) -> Result<Self, Error> {
        PlainText::new(text).map(Self::Text)
    }

    fn with_id(self, id: BlockId) -> Result<PlainBlock, Error> {
        match self {
            ToolBlock::Text(text) => TextBlock::new(id, text.as_str()).map(PlainBlock::Text),
            ToolBlock::Image(source) => Ok(PlainBlock::Image(ImageBlock::new(id, source))),
            ToolBlock::Structured(value) => {
                StructuredBlock::new(id, value).map(PlainBlock::StructuredContent)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolContent {
    content: Vec<ToolBlock>,
    is_error: bool,
}

impl ToolContent {
    pub fn new(content: Vec<ToolBlock>, is_error: bool) -> Self {
        Self { content, is_error }
    }

    pub fn error_text(text: impl Into<String>) -> Result<Self, Error> {
        Ok(Self {
            content: vec![ToolBlock::text(text)?],
            is_error: true,
        })
    }

    pub fn from_port_error(error: PortError) -> Result<Self, ToolFault> {
        match error {
            PortError::Refused(reason) => Self::error_text(reason.into_string())
                .or_else(|_| Self::error_text(BLANK_REFUSAL))
                .map_err(ToolFault::new),
            PortError::Fault(cause) => Err(ToolFault::new(cause)),
        }
    }

    pub fn content(&self) -> &[ToolBlock] {
        &self.content
    }

    pub fn is_error(&self) -> bool {
        self.is_error
    }

    pub fn into_result(
        self,
        id: BlockId,
        call: BlockId,
        name: ToolName,
        mut block_ids: impl FnMut() -> BlockId,
    ) -> Result<RuntimeToolResult, Error> {
        let content = self
            .content
            .into_iter()
            .map(|block| block.with_id(block_ids()))
            .collect::<Result<Vec<_>, _>>()?;
        RuntimeToolResult::new(id, call, name, self.is_error, content)
    }
}

#[derive(Clone)]
pub struct Tool {
    spec: ToolSpec,
    code: Arc<dyn ToolCode>,
}

impl Tool {
    pub fn new(spec: ToolSpec, code: Arc<dyn ToolCode>) -> Self {
        Self { spec, code }
    }

    pub fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    pub fn name(&self) -> &ToolName {
        self.spec.name()
    }

    pub fn code(&self) -> &Arc<dyn ToolCode> {
        &self.code
    }

    pub async fn call(
        &self,
        arguments: &OpaqueJson,
        cancel: CancelView,
        result: &mut dyn ResultStream,
    ) -> Result<ToolContent, ToolFault> {
        self.code.call(arguments, cancel, result).await
    }
}

impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool").field("name", self.name()).finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the tool name {name} appears twice among the tools of an instance")]
pub struct DuplicateTool {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ToolSet {
    tools: Vec<Tool>,
}

impl ToolSet {
    pub fn new(tools: Vec<Tool>, mail_tools: Vec<Tool>) -> Result<Self, DuplicateTool> {
        let mut seen = HashSet::new();
        let all: Vec<Tool> = tools.into_iter().chain(mail_tools).collect();
        for tool in &all {
            if !seen.insert(tool.name().as_str().to_owned()) {
                return Err(DuplicateTool {
                    name: tool.name().as_str().to_owned(),
                });
            }
        }
        Ok(Self { tools: all })
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    pub fn find(&self, name: &str) -> Option<&Tool> {
        self.tools.iter().find(|tool| tool.name().as_str() == name)
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.iter().map(|tool| tool.spec().clone()).collect()
    }
}
