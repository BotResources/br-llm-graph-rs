use std::sync::Arc;

use br_agentic_contract::memory::{FixedThreadPort, FixedToolsPort};
use br_agentic_contract::{AgentPort, IncomingPort, ThreadAccess, ThreadPort, ToolsPort};
use br_agentic_contract::{PromptPort, ReactOutcomePort};
use br_agentic_nodes::{AuthorSource, FallbackList, Ports, ReactAgent};

use crate::common::{Doc, Rig, Scripted, list};

pub struct Parts {
    pub thread: Arc<dyn ThreadPort<Doc>>,
    pub agent: Arc<dyn AgentPort<Doc>>,
    pub access: Arc<dyn ThreadAccess>,
    pub prompt: Arc<dyn PromptPort<Doc>>,
    pub tools: Arc<dyn ToolsPort<Doc>>,
    pub incoming: Option<Arc<dyn IncomingPort>>,
}

impl Parts {
    pub fn of(rig: &Rig) -> Self {
        Self {
            thread: Arc::new(FixedThreadPort::new(&rig.record, rig.thread)),
            agent: rig.agents.clone(),
            access: rig.access.clone(),
            prompt: rig.prompts.clone(),
            tools: Arc::new(FixedToolsPort::new(&rig.record, rig.tools.clone())),
            incoming: rig
                .with_incoming
                .then(|| rig.incoming.clone() as Arc<dyn IncomingPort>),
        }
    }

    pub fn with_access(mut self, access: Arc<dyn ThreadAccess>) -> Self {
        self.access = access;
        self
    }

    pub fn ports(self) -> Ports<Doc> {
        let ports = Ports::new(
            self.thread,
            self.agent,
            self.access,
            self.prompt,
            self.tools,
        );
        match self.incoming {
            Some(incoming) => ports.with_incoming(incoming),
            None => ports,
        }
    }
}

pub fn react_over(rig: &Rig, parts: Parts, clients: &[&Arc<Scripted>]) -> ReactAgent<Doc> {
    react_spec(rig, parts, list(clients), rig.outcome.clone(), None)
}

pub fn react_list(rig: &Rig, parts: Parts, fallback: FallbackList) -> ReactAgent<Doc> {
    react_spec(rig, parts, fallback, rig.outcome.clone(), None)
}

pub fn react_spec(
    rig: &Rig,
    parts: Parts,
    fallback: FallbackList,
    outcome: Arc<dyn ReactOutcomePort<Doc>>,
    author: Option<AuthorSource<Doc>>,
) -> ReactAgent<Doc> {
    let mut spec = rig.react_spec(fallback);
    spec.outcome = outcome;
    if let Some(author) = author {
        spec.author = author;
    }
    ReactAgent::new("agent", parts.ports(), rig.settings.clone(), spec)
}
