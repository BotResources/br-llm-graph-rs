use std::sync::Arc;

use br_agentic_contract::{Fields, IncomingPort, PromptPort, ThreadAccess, ToolsPort, WebAccess};
use br_agentic_harness_contract::{HarnessHistory, MailPort, SandboxPort, SkillsPort};

use crate::history::HistoryAdapter;
use crate::incoming::IncomingAdapter;
use crate::prompt::PromptAdapter;
use crate::shared::Shared;
use crate::tools::ToolsAdapter;

pub struct HarnessBuilder<F: Fields> {
    history: Arc<dyn HarnessHistory>,
    prompt: Arc<dyn PromptPort<F>>,
    tools: Arc<dyn ToolsPort<F>>,
    mail: Option<Arc<dyn MailPort>>,
    skills: Option<Arc<dyn SkillsPort>>,
    sandbox: Option<Arc<dyn SandboxPort>>,
    web: Option<WebAccess>,
}

impl<F: Fields> HarnessBuilder<F> {
    pub fn mail(mut self, mail: Arc<dyn MailPort>) -> Self {
        self.mail = Some(mail);
        self
    }

    pub fn skills(mut self, skills: Arc<dyn SkillsPort>) -> Self {
        self.skills = Some(skills);
        self
    }

    pub fn sandbox(mut self, sandbox: Arc<dyn SandboxPort>) -> Self {
        self.sandbox = Some(sandbox);
        self
    }

    pub fn web_access(mut self, web: WebAccess) -> Self {
        self.web = Some(web);
        self
    }

    pub fn build(self) -> Harness<F> {
        let shared = Arc::new(Shared::default());
        Harness {
            history: Arc::new(HistoryAdapter::new(
                Arc::clone(&self.history),
                Arc::clone(&shared),
            )),
            prompt: Arc::new(PromptAdapter::new(self.prompt)),
            tools: Arc::new(ToolsAdapter::new(
                self.tools,
                self.mail.clone(),
                self.sandbox.clone(),
                Arc::clone(&shared),
            )),
            incoming: self
                .mail
                .map(|mail| Arc::new(IncomingAdapter::new(self.history, mail))),
            skills: self.skills,
            sandbox: self.sandbox,
            web: self.web,
            shared,
        }
    }
}

pub struct Harness<F: Fields> {
    history: Arc<HistoryAdapter>,
    prompt: Arc<PromptAdapter<F>>,
    tools: Arc<ToolsAdapter<F>>,
    incoming: Option<Arc<IncomingAdapter>>,
    skills: Option<Arc<dyn SkillsPort>>,
    pub(crate) sandbox: Option<Arc<dyn SandboxPort>>,
    pub(crate) web: Option<WebAccess>,
    pub(crate) shared: Arc<Shared>,
}

impl<F: Fields> Harness<F> {
    pub fn builder(
        history: Arc<dyn HarnessHistory>,
        prompt: Arc<dyn PromptPort<F>>,
        tools: Arc<dyn ToolsPort<F>>,
    ) -> HarnessBuilder<F> {
        HarnessBuilder {
            history,
            prompt,
            tools,
            mail: None,
            skills: None,
            sandbox: None,
            web: None,
        }
    }

    pub fn history(&self) -> Arc<dyn ThreadAccess> {
        self.history.clone()
    }

    pub fn prompt(&self) -> Arc<dyn PromptPort<F>> {
        self.prompt.clone()
    }

    pub fn tools(&self) -> Arc<dyn ToolsPort<F>> {
        self.tools.clone()
    }

    pub fn incoming(&self) -> Option<Arc<dyn IncomingPort>> {
        self.incoming
            .clone()
            .map(|incoming| incoming as Arc<dyn IncomingPort>)
    }

    pub fn skills(&self) -> Option<&Arc<dyn SkillsPort>> {
        self.skills.as_ref()
    }
}
