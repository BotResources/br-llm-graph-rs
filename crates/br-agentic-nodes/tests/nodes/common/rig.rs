use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::{
    FixedThreadPort, FixedToolsPort, MemoryAgentPort, MemoryHub, MemoryWorkspace,
};
use br_agentic_contract::{
    AgentLabel, AgentPort, FileSystemMode, InboxEntry, Incoming, PortFault, ThreadAccess,
    ThreadRef, Tool, WebAccess, WorkspaceFile,
};
use br_agentic_nodes::{
    CriticRequest, CriticRequestSource, FallbackList, GeneratorCritic, GeneratorCriticSpec, Ladder,
    ModelClient, Ports, PromptPort, ReactAgent, RoundLimit, Settings, StepLimit, Timeouts,
};
use br_graph_contract::InstanceId;
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{AgentId, HumanId, HumanMessage, IncomingEntry, PlainText, TurnId};

use super::ids::{author, entry_id, human_id, instance_id, text_block, turn_id};
use super::{Doc, Recording, Scripted};

#[derive(Default)]
pub struct Prompts {
    asked: Mutex<HashMap<String, usize>>,
}

impl Prompts {
    pub fn asked(&self, label: &str) -> usize {
        self.asked.lock().unwrap().get(label).copied().unwrap_or(0)
    }
}

impl PromptPort<Doc> for Prompts {
    fn prompt<'a>(
        &'a self,
        _snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> BoxFuture<'a, Result<PlainText, PortFault>> {
        Box::pin(async move {
            *self
                .asked
                .lock()
                .unwrap()
                .entry(label.as_str().to_owned())
                .or_default() += 1;
            Ok(PlainText::new(format!("system prompt of the {label}")).unwrap())
        })
    }
}

pub struct Rig {
    pub hub: MemoryHub,
    pub thread: ThreadRef,
    pub outcome: Arc<Recording>,
    pub workspace: Arc<MemoryWorkspace>,
    pub prompts: Arc<Prompts>,
    pub agents: Arc<MemoryAgentPort<Doc>>,
    pub tools: Vec<Tool>,
    pub inbox: bool,
    pub settings: Settings,
}

pub fn client(scripted: &Arc<Scripted>) -> ModelClient {
    ModelClient::new(scripted.clone())
}

pub fn list(clients: &[&Arc<Scripted>]) -> FallbackList {
    FallbackList::new(clients.iter().map(|scripted| client(scripted)).collect()).unwrap()
}

pub fn ladder(entries: Vec<FallbackList>) -> Ladder {
    Ladder::new(entries).unwrap()
}

pub fn one(scripted: &Arc<Scripted>) -> Ladder {
    ladder(vec![list(&[scripted])])
}

pub fn doc() -> Doc {
    Doc {
        note: "start".to_owned(),
    }
}

impl Rig {
    pub fn new() -> Self {
        Self::with_authors(true)
    }

    pub fn without_authors() -> Self {
        Self::with_authors(false)
    }

    fn with_authors(named: bool) -> Self {
        let hub = MemoryHub::new();
        let thread = hub.create_thread();
        let mut agents = MemoryAgentPort::new(hub.clone());
        if named {
            for label in ["agent", "generator", "critic"] {
                agents = agents.with_author(AgentLabel::new(label).unwrap(), author(label));
            }
        }
        Self {
            thread,
            outcome: Arc::new(Recording::default()),
            workspace: Arc::new(MemoryWorkspace::new(Vec::new())),
            prompts: Arc::new(Prompts::default()),
            agents: Arc::new(agents),
            tools: Vec::new(),
            inbox: false,
            settings: Settings::new(
                StepLimit::new(20).unwrap(),
                Timeouts::new(Duration::from_secs(5), Duration::from_secs(5)).unwrap(),
                WebAccess::Full,
                FileSystemMode::Never,
            ),
            hub,
        }
    }

    pub fn with_inbox(mut self) -> Self {
        self.inbox = true;
        self
    }

    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_step_limit(mut self, limit: u32) -> Self {
        self.settings.step_limit = StepLimit::new(limit).unwrap();
        self
    }

    pub fn with_timeouts(mut self, model: Duration, tool: Duration) -> Self {
        self.settings.timeouts = Timeouts::new(model, tool).unwrap();
        self
    }

    pub fn with_files(mut self, mode: FileSystemMode, files: Vec<WorkspaceFile>) -> Self {
        self.settings.file_system = mode;
        self.workspace = Arc::new(MemoryWorkspace::new(files));
        self
    }

    pub fn ports(&self) -> Ports<Doc> {
        let ports = Ports::new(
            Arc::new(FixedThreadPort::new(self.thread)),
            self.agents.clone(),
            Arc::new(self.hub.thread_access()),
            self.prompts.clone(),
            Arc::new(FixedToolsPort::new(self.tools.clone())),
            self.workspace.clone(),
            self.outcome.clone(),
        );
        if self.inbox {
            ports.with_inbox(Arc::new(self.hub.inbox()))
        } else {
            ports
        }
    }

    pub fn react(&self, clients: &[&Arc<Scripted>]) -> ReactAgent<Doc> {
        ReactAgent::new("agent", self.ports(), list(clients), self.settings.clone())
    }

    pub fn generator_critic(
        &self,
        generator: Ladder,
        critic: Ladder,
        round_limit: u32,
    ) -> GeneratorCritic<Doc> {
        GeneratorCritic::new(
            "pair",
            self.ports(),
            self.settings.clone(),
            GeneratorCriticSpec {
                generator,
                critic,
                round_limit: RoundLimit::new(round_limit).unwrap(),
                critic_request: CriticRequestSource::Fixed(critic_request()),
            },
        )
    }

    pub async fn agent(&self, label: &str) -> AgentId {
        let label = AgentLabel::new(label).unwrap();
        self.agents
            .agent(&doc(), instance_id(), &label)
            .await
            .unwrap()
            .id
    }

    pub async fn start_with(&self, text: &str) -> TurnId {
        let agent = self.agent("agent").await;
        self.start_as(agent, text).await
    }

    pub async fn start_as(&self, agent: AgentId, text: &str) -> TurnId {
        let turn = turn_id();
        let message = HumanMessage::new(entry_id(), human_id(), vec![text_block(text)]).unwrap();
        let entry = InboxEntry::new(IncomingEntry::from(message)).unwrap();
        self.hub
            .thread_access()
            .add_input_turn(&self.thread, agent, turn, vec![Incoming::Entry(entry)])
            .await
            .unwrap();
        turn
    }

    pub fn deliver_human(&self, agent: AgentId, text: &str) -> br_llm_messages::EntryId {
        let message = HumanMessage::new(entry_id(), human_id(), vec![text_block(text)]).unwrap();
        let id = message.id();
        self.hub.deliver_entry(
            agent,
            InboxEntry::new(IncomingEntry::from(message)).unwrap(),
        );
        id
    }

    pub fn instance(&self) -> InstanceId {
        instance_id()
    }
}

pub fn critic_request() -> CriticRequest {
    CriticRequest {
        text: PlainText::new("what do you think of this?").unwrap(),
        human: HumanId::from_uuid(super::ids::uuid()).unwrap(),
    }
}

impl Rig {
    pub fn context_of(&self, agent: AgentId) -> br_agentic_contract::InboxContext {
        br_agentic_contract::InboxContext {
            instance: instance_id(),
            thread: self.thread,
            agent,
        }
    }

    pub async fn send_mail(
        &self,
        from: AgentId,
        to: AgentId,
        subject: &str,
        attachments: Vec<br_agentic_contract::Attachment>,
    ) -> br_agentic_contract::MailId {
        use br_agentic_contract::InboxPort;
        let id = super::ids::mail_id();
        self.hub
            .inbox()
            .send(
                &self.context_of(from),
                br_agentic_contract::NewMail {
                    id,
                    to: vec![to],
                    cc: Vec::new(),
                    subject: subject.to_owned(),
                    body: "the body".to_owned(),
                    attachments,
                },
            )
            .await
            .unwrap();
        id
    }
}

impl Rig {
    pub fn deliver_human_later(&self, _agent: AgentId, text: &str) -> InboxEntry {
        let message = HumanMessage::new(entry_id(), human_id(), vec![text_block(text)]).unwrap();
        InboxEntry::new(IncomingEntry::from(message)).unwrap()
    }
}

impl Rig {
    pub async fn start_gc(&self, text: &str) -> AgentId {
        let agent = self.agent("generator").await;
        self.start_as(agent, text).await;
        agent
    }
}

impl Rig {
    pub fn note_on_first_call(&self, model: &Arc<Scripted>, agent: AgentId, text: &str) {
        let hub = self.hub.clone();
        let note = self.deliver_human_later(agent, text);
        let delivered = std::sync::atomic::AtomicBool::new(false);
        model.on_call(move |_| {
            if !delivered.swap(true, std::sync::atomic::Ordering::SeqCst) {
                hub.deliver_entry(agent, note.clone());
            }
        });
    }
}
