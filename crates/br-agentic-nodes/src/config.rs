use std::sync::Arc;

use br_agentic_contract::{
    AgentLabel, AgentPort, AuthorPort, IncomingPort, OutputSchemaPort, PromptPort, ThreadAccess,
    ThreadPort, ToolsPort, WebAccess,
};
use br_graph_contract::Fields;
use br_llm_messages::Author;

use crate::fault::Fault;
use crate::limits::{StepLimit, Timeouts};

pub struct Ports<F: Fields> {
    pub(crate) thread: Arc<dyn ThreadPort<F>>,
    pub(crate) agent: Arc<dyn AgentPort<F>>,
    pub(crate) access: Arc<dyn ThreadAccess>,
    pub(crate) prompt: Arc<dyn PromptPort<F>>,
    pub(crate) tools: Arc<dyn ToolsPort<F>>,
    pub(crate) incoming: Option<Arc<dyn IncomingPort>>,
    pub(crate) output_schema: Option<Arc<dyn OutputSchemaPort<F>>>,
}

impl<F: Fields> Ports<F> {
    pub fn new(
        thread: Arc<dyn ThreadPort<F>>,
        agent: Arc<dyn AgentPort<F>>,
        access: Arc<dyn ThreadAccess>,
        prompt: Arc<dyn PromptPort<F>>,
        tools: Arc<dyn ToolsPort<F>>,
    ) -> Self {
        Self {
            thread,
            agent,
            access,
            prompt,
            tools,
            incoming: None,
            output_schema: None,
        }
    }

    #[must_use]
    pub fn with_incoming(mut self, incoming: Arc<dyn IncomingPort>) -> Self {
        self.incoming = Some(incoming);
        self
    }

    #[must_use]
    pub fn with_output_schema(mut self, output_schema: Arc<dyn OutputSchemaPort<F>>) -> Self {
        self.output_schema = Some(output_schema);
        self
    }

    pub fn has_incoming(&self) -> bool {
        self.incoming.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub step_limit: StepLimit,
    pub timeouts: Timeouts,
    pub web_access: WebAccess,
}

impl Settings {
    pub fn new(step_limit: StepLimit, timeouts: Timeouts, web_access: WebAccess) -> Self {
        Self {
            step_limit,
            timeouts,
            web_access,
        }
    }
}

pub enum AuthorSource<F: Fields> {
    Fixed(Author),
    Port(Arc<dyn AuthorPort<F>>),
}

impl<F: Fields> Clone for AuthorSource<F> {
    fn clone(&self) -> Self {
        match self {
            AuthorSource::Fixed(author) => AuthorSource::Fixed(author.clone()),
            AuthorSource::Port(port) => AuthorSource::Port(Arc::clone(port)),
        }
    }
}

impl<F: Fields> AuthorSource<F> {
    pub(crate) async fn resolve(&self, snapshot: &F, label: &AgentLabel) -> Result<Author, Fault> {
        match self {
            AuthorSource::Fixed(author) => Ok(author.clone()),
            AuthorSource::Port(port) => {
                let name = port.author(snapshot, label).await?;
                Author::new(name).map_err(Fault::value)
            }
        }
    }
}

pub(crate) struct Common<F: Fields> {
    pub(crate) name: String,
    pub(crate) ports: Ports<F>,
    pub(crate) settings: Settings,
}
