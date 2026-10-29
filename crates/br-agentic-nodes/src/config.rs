use std::sync::Arc;

use br_agentic_contract::{
    AgentPort, FileSystemMode, InboxPort, ThreadAccess, ThreadPort, ToolsPort, WebAccess,
    WorkspacePort,
};
use br_graph_contract::Fields;

use crate::limits::{StepLimit, Timeouts};
use crate::ports::{OutcomePort, PromptPort};

pub struct Ports<F: Fields> {
    pub(crate) thread: Arc<dyn ThreadPort<F>>,
    pub(crate) agent: Arc<dyn AgentPort<F>>,
    pub(crate) access: Arc<dyn ThreadAccess>,
    pub(crate) prompt: Arc<dyn PromptPort<F>>,
    pub(crate) tools: Arc<dyn ToolsPort<F>>,
    pub(crate) workspace: Arc<dyn WorkspacePort<F>>,
    pub(crate) outcome: Arc<dyn OutcomePort<F>>,
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
        outcome: Arc<dyn OutcomePort<F>>,
    ) -> Self {
        Self {
            thread,
            agent,
            access,
            prompt,
            tools,
            workspace,
            outcome,
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

pub(crate) struct Common<F: Fields> {
    pub(crate) name: String,
    pub(crate) ports: Ports<F>,
    pub(crate) settings: Settings,
}
