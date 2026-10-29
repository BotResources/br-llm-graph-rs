use br_agentic_contract::memory::{MemoryHub, MemoryInbox, MemoryThreadAccess};
use br_agentic_contract::{
    InboxContext, InboxEntry, InboxItem, Incoming, MailId, MailItem, NewMail, ThreadAccess,
    ThreadRef, TurnTarget,
};
use br_graph_contract::{Fields, InstanceId, Reason};
use br_llm_messages::{
    AgentId, Author, BlockId, ContentBlock, EntryId, HumanId, HumanMessage, IncomingEntry,
    ModelResponse, OpaqueJson, PlainBlock, ProjectedThread, ResponseEnding, ResponseMetadata,
    RuntimeNotice, RuntimeToolCall, RuntimeToolResult, TextBlock, ToolName, TurnId, Uuid,
};
use serde::{Deserialize, Serialize};

pub fn uuid() -> Uuid {
    Uuid::now_v7()
}

pub fn entry_id() -> EntryId {
    EntryId::from_uuid(uuid()).unwrap()
}

pub fn turn_id() -> TurnId {
    TurnId::from_uuid(uuid()).unwrap()
}

pub fn block_id() -> BlockId {
    BlockId::from_uuid(uuid()).unwrap()
}

pub fn agent_id() -> AgentId {
    AgentId::from_uuid(uuid()).unwrap()
}

pub fn mail_id() -> MailId {
    MailId::new(entry_id())
}

pub fn instance_id() -> InstanceId {
    InstanceId::new(uuid()).unwrap()
}

pub fn author(name: &str) -> Author {
    Author::new(name).unwrap()
}

pub fn text_block(text: &str) -> PlainBlock {
    PlainBlock::Text(TextBlock::new(block_id(), text).unwrap())
}

pub fn human(text: &str) -> InboxEntry {
    let message = HumanMessage::new(
        entry_id(),
        HumanId::from_uuid(uuid()).unwrap(),
        vec![text_block(text)],
    )
    .unwrap();
    InboxEntry::new(IncomingEntry::from(message)).unwrap()
}

pub fn notice(text: &str) -> InboxEntry {
    InboxEntry::new(IncomingEntry::from(
        RuntimeNotice::new(entry_id(), text).unwrap(),
    ))
    .unwrap()
}

pub fn response_text(entry: EntryId, text: &str) -> ModelResponse {
    ModelResponse::new(
        entry,
        vec![ContentBlock::Text(
            TextBlock::new(block_id(), text).unwrap(),
        )],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )
    .unwrap()
}

pub fn response_call(entry: EntryId, call: BlockId, name: &str) -> ModelResponse {
    let call = RuntimeToolCall::new(
        call,
        ToolName::new(name).unwrap(),
        OpaqueJson::parse("{}").unwrap(),
    )
    .unwrap();
    ModelResponse::new(
        entry,
        vec![ContentBlock::RuntimeToolCall(call)],
        ResponseEnding::tool_use(),
        ResponseMetadata::new(),
    )
    .unwrap()
}

pub fn tool_result(call: BlockId, name: &str, text: &str) -> RuntimeToolResult {
    RuntimeToolResult::new(
        block_id(),
        call,
        ToolName::new(name).unwrap(),
        false,
        vec![text_block(text)],
    )
    .unwrap()
}

pub fn new_mail(to: &[AgentId], cc: &[AgentId], subject: &str) -> NewMail {
    NewMail {
        id: mail_id(),
        to: to.to_vec(),
        cc: cc.to_vec(),
        subject: subject.to_owned(),
        body: format!("body of {subject}"),
        attachments: Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Doc {
    pub n: i64,
}

impl Fields for Doc {
    type Update = i64;

    fn empty_update() -> i64 {
        0
    }

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, update: i64) -> Result<(), Reason> {
        self.n += update;
        Ok(())
    }
}

pub struct World {
    pub hub: MemoryHub,
    pub access: MemoryThreadAccess,
    pub inbox: MemoryInbox,
    pub thread: ThreadRef,
    pub agent: AgentId,
    pub name: Author,
}

impl World {
    pub fn new() -> Self {
        let hub = MemoryHub::new();
        let thread = hub.create_thread();
        let agent = agent_id();
        let name = author("agent");
        hub.set_author(agent, name.clone());
        Self {
            access: hub.thread_access(),
            inbox: hub.inbox(),
            hub,
            thread,
            agent,
            name,
        }
    }

    pub fn context(&self) -> InboxContext {
        InboxContext {
            instance: instance_id(),
            thread: self.thread,
            agent: self.agent,
        }
    }

    pub fn context_of(&self, agent: AgentId) -> InboxContext {
        InboxContext {
            instance: instance_id(),
            thread: self.thread,
            agent,
        }
    }

    pub fn ai(&self, turn: TurnId) -> TurnTarget {
        TurnTarget::Ai {
            id: turn,
            author: self.name.clone(),
        }
    }

    pub async fn start_with_human(&self, text: &str) -> TurnId {
        let turn = turn_id();
        self.access
            .add_input_turn(
                &self.thread,
                self.agent,
                turn,
                vec![Incoming::Entry(human(text))],
            )
            .await
            .unwrap();
        turn
    }

    pub async fn open_ai_turn_with_call(&self, call: BlockId) -> TurnId {
        self.start_with_human("hello").await;
        let turn = turn_id();
        self.access
            .complete_response(
                &self.thread,
                self.agent,
                self.ai(turn),
                response_call(entry_id(), call, "search"),
            )
            .await
            .unwrap();
        turn
    }

    pub async fn projected(&self) -> ProjectedThread {
        self.access.load(&self.thread, self.agent).await.unwrap()
    }

    pub async fn projected_of(&self, agent: AgentId) -> ProjectedThread {
        self.access.load(&self.thread, agent).await.unwrap()
    }
}

pub fn mail_to(item: &InboxItem) -> &MailItem {
    match item {
        InboxItem::Mail(mail) => mail,
        InboxItem::Entry(_) => panic!("expected a mail item"),
    }
}

pub fn incoming_of(thread: &ProjectedThread, id: EntryId) -> IncomingEntry {
    thread
        .entries()
        .find_map(|placed| match placed.into_entry() {
            br_llm_messages::EntryView::Incoming(entry) if entry.id() == id => Some(entry.clone()),
            _ => None,
        })
        .unwrap()
}

pub fn entry_ids(thread: &ProjectedThread) -> Vec<EntryId> {
    thread.entries().map(|placed| placed.entry().id()).collect()
}

pub fn mail_item_to(sender: AgentId, to: &[AgentId], subject: &str, names: &[&str]) -> MailItem {
    MailItem::To {
        id: mail_id(),
        sender,
        to: to.to_vec(),
        cc: Vec::new(),
        subject: subject.to_owned(),
        body: format!("body of {subject}"),
        attachment_names: names.iter().map(|name| (*name).to_owned()).collect(),
    }
}
