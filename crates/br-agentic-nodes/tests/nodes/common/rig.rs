use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::{
    FixedThreadPort, FixedToolsPort, MemoryAgentPort, MemoryInbox, MemoryThreadAccess,
    MemoryWorkspace, Record,
};
use br_agentic_contract::{
    AgentLabel, FileSystemMode, InboxEntry, PortError, PortFuture, ThreadRef, Tool, WebAccess,
    WorkspaceFile,
};
use br_agentic_nodes::{
    AuthorSource, CriticRequest, CriticRequestSource, FallbackList, GeneratorCritic,
    GeneratorCriticSpec, Ladder, ModelClient, Ports, PromptPort, ReactAgent, ReactSpec, RoundLimit,
    Settings, StepLimit, ThinkingExpectation, Timeouts,
};
use br_llm_messages::{
    AgentId, Conversation, ConversationId, HumanId, HumanMessage, IncomingEntry, InputTurn, Turn,
};

use super::ids::{author, entry_id, human_id, text_block, turn_id, uuid};
use super::{Doc, Recording, Scripted};

#[derive(Default)]
pub struct Prompts {
    asked: Mutex<HashMap<String, usize>>,
    blank: bool,
}

impl Prompts {
    pub fn blank() -> Self {
        Self {
            asked: Mutex::default(),
            blank: true,
        }
    }

    pub fn asked(&self, label: &str) -> usize {
        self.asked.lock().unwrap().get(label).copied().unwrap_or(0)
    }
}

impl PromptPort<Doc> for Prompts {
    fn prompt<'a>(
        &'a self,
        _snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move {
            *self
                .asked
                .lock()
                .unwrap()
                .entry(label.as_str().to_owned())
                .or_default() += 1;
            if self.blank {
                return Ok("   ".to_owned());
            }
            Ok(format!("system prompt of the {label}"))
        })
    }
}

pub struct Rig {
    pub record: Record,
    pub thread: ThreadRef,
    pub access: Arc<MemoryThreadAccess>,
    pub inbox: Arc<MemoryInbox>,
    pub outcome: Arc<Recording>,
    pub workspace: Arc<MemoryWorkspace>,
    pub prompts: Arc<Prompts>,
    pub agents: Arc<MemoryAgentPort<Doc>>,
    pub tools: Vec<Tool>,
    pub with_inbox: bool,
    pub settings: Settings,
    pub agent: AgentId,
    pub generator: AgentId,
    pub critic: AgentId,
    pub programmatic: AgentId,
    pub conversation_id: ConversationId,
}

pub fn client(scripted: &Arc<Scripted>) -> ModelClient {
    ModelClient::new(scripted.clone(), ThinkingExpectation::NotExpected)
}

pub fn thinking_client(scripted: &Arc<Scripted>) -> ModelClient {
    ModelClient::new(scripted.clone(), ThinkingExpectation::Expected)
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

pub fn agent_id() -> AgentId {
    AgentId::from_uuid(uuid()).unwrap()
}

pub fn human_message(text: &str) -> HumanMessage {
    HumanMessage::new(entry_id(), human_id(), vec![text_block(text)]).unwrap()
}

pub fn inbox_entry(text: &str) -> InboxEntry {
    InboxEntry::new(IncomingEntry::from(human_message(text))).unwrap()
}

pub fn conversation_with(id: ConversationId, first: &str) -> Conversation {
    let mut conversation = Conversation::new(id);
    let turn = InputTurn::new(turn_id(), vec![IncomingEntry::from(human_message(first))]).unwrap();
    conversation.push_turn(Turn::from(turn)).unwrap();
    conversation
}

pub fn critic_request() -> CriticRequest {
    CriticRequest {
        text: "what do you think of this?".to_owned(),
        human: HumanId::from_uuid(uuid()).unwrap(),
    }
}

impl Rig {
    pub fn new() -> Self {
        let conversation_id = ConversationId::from_uuid(uuid()).unwrap();
        Self::with_conversation(conversation_id, conversation_with(conversation_id, "start"))
    }

    pub fn empty() -> Self {
        let conversation_id = ConversationId::from_uuid(uuid()).unwrap();
        Self::with_conversation(conversation_id, Conversation::new(conversation_id))
    }

    pub fn with_conversation(conversation_id: ConversationId, conversation: Conversation) -> Self {
        let record = Record::new();
        let thread = ThreadRef::from_uuid(uuid()).unwrap();
        let agent = agent_id();
        let generator = agent_id();
        let critic = agent_id();
        let programmatic = agent_id();
        let mut agents = MemoryAgentPort::new(&record);
        for (label, id) in [
            ("agent", agent),
            ("generator", generator),
            ("critic", critic),
            ("programmatic_critic", programmatic),
        ] {
            agents = agents.with_agent(AgentLabel::new(label).unwrap(), id);
        }
        Self {
            access: Arc::new(MemoryThreadAccess::new(&record, thread, conversation)),
            inbox: Arc::new(MemoryInbox::new(&record)),
            outcome: Arc::new(Recording::default()),
            workspace: Arc::new(MemoryWorkspace::new(&record, Vec::new())),
            prompts: Arc::new(Prompts::default()),
            agents: Arc::new(agents),
            tools: Vec::new(),
            with_inbox: false,
            settings: Settings::new(
                StepLimit::new(20).unwrap(),
                Timeouts::new(Duration::from_secs(5), Duration::from_secs(5)).unwrap(),
                WebAccess::Full,
                FileSystemMode::Never,
            ),
            record,
            thread,
            agent,
            generator,
            critic,
            programmatic,
            conversation_id,
        }
    }

    pub fn with_inbox(mut self) -> Self {
        self.with_inbox = true;
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
        self.workspace = Arc::new(MemoryWorkspace::new(&self.record, files));
        self
    }

    pub fn with_outcome(mut self, outcome: Recording) -> Self {
        self.outcome = Arc::new(outcome);
        self
    }

    pub fn with_prompts(mut self, prompts: Prompts) -> Self {
        self.prompts = Arc::new(prompts);
        self
    }

    pub fn ports(&self) -> Ports<Doc> {
        let ports = Ports::new(
            Arc::new(FixedThreadPort::new(&self.record, self.thread)),
            self.agents.clone(),
            self.access.clone(),
            self.prompts.clone(),
            Arc::new(FixedToolsPort::new(&self.record, self.tools.clone())),
            self.workspace.clone(),
        );
        if self.with_inbox {
            ports.with_inbox(self.inbox.clone())
        } else {
            ports
        }
    }

    pub fn react_spec(&self, fallback: FallbackList) -> ReactSpec<Doc> {
        ReactSpec {
            fallback,
            outcome: self.outcome.clone(),
            author: AuthorSource::Fixed(author("Agent")),
        }
    }

    pub fn react(&self, clients: &[&Arc<Scripted>]) -> ReactAgent<Doc> {
        ReactAgent::new(
            "agent",
            self.ports(),
            self.settings.clone(),
            self.react_spec(list(clients)),
        )
    }

    pub fn pair_spec(
        &self,
        generator: Ladder,
        critic: Ladder,
        round_limit: u32,
    ) -> GeneratorCriticSpec<Doc> {
        GeneratorCriticSpec {
            generator,
            critic,
            round_limit: RoundLimit::new(round_limit).unwrap(),
            critic_request: CriticRequestSource::Fixed(critic_request()),
            generator_author: AuthorSource::Fixed(author("Generator")),
            critic_author: AuthorSource::Fixed(author("Critic")),
            outcome: self.outcome.clone(),
        }
    }

    pub fn pair(
        &self,
        generator: Ladder,
        critic: Ladder,
        round_limit: u32,
    ) -> GeneratorCritic<Doc> {
        GeneratorCritic::new(
            "pair",
            self.ports(),
            self.settings.clone(),
            self.pair_spec(generator, critic, round_limit),
        )
    }

    pub fn conversation(&self) -> Conversation {
        self.access.conversation()
    }
}

impl Default for Rig {
    fn default() -> Self {
        Self::new()
    }
}
