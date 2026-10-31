use std::sync::Arc;

use br_agentic_contract::{
    AgentLabel, AgentPort, FileSystemMode, InboxPort, ThreadAccess, ThreadPort, ToolsPort,
    WebAccess, WorkspacePort,
};
use br_graph_contract::Fields;
use br_llm_messages::Author;

use crate::fault::Fault;
use crate::limits::{StepLimit, Timeouts};
use crate::ports::{AuthorPort, PromptPort};

pub struct Ports<F: Fields> {
    pub(crate) thread: Arc<dyn ThreadPort<F>>,
    pub(crate) agent: Arc<dyn AgentPort<F>>,
    pub(crate) access: Arc<dyn ThreadAccess>,
    pub(crate) prompt: Arc<dyn PromptPort<F>>,
    pub(crate) tools: Arc<dyn ToolsPort<F>>,
    pub(crate) workspace: Arc<dyn WorkspacePort<F>>,
    pub(crate) inbox: Option<Arc<dyn InboxPort>>,
}

impl<F: Fields> Ports<F> {
    pub fn new(
        thread: Arc<dyn ThreadPort<F>>,
        agent: Arc<dyn AgentPort<F>>,
        access: Arc<dyn ThreadAccess>,
        prompt: Arc<dyn PromptPort<F>>,
        tools: Arc<dyn ToolsPort<F>>,
        workspace: Arc<dyn WorkspacePort<F>>,
    ) -> Self {
        Self {
            thread,
            agent,
            access,
            prompt,
            tools,
            workspace,
            inbox: None,
        }
    }

    #[must_use]
    pub fn with_inbox(mut self, inbox: Arc<dyn InboxPort>) -> Self {
        self.inbox = Some(inbox);
        self
    }

    pub fn has_inbox(&self) -> bool {
        self.inbox.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub step_limit: StepLimit,
    pub timeouts: Timeouts,
    pub web_access: WebAccess,
    pub file_system: FileSystemMode,
}

impl Settings {
    pub fn new(
        step_limit: StepLimit,
        timeouts: Timeouts,
        web_access: WebAccess,
        file_system: FileSystemMode,
    ) -> Self {
        Self {
            step_limit,
            timeouts,
            web_access,
            file_system,
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
