use std::num::NonZeroUsize;

use br_agentic_contract::ThreadAccess;
use br_agentic_contract::memory::{MemoryIncoming, MemoryThreadAccess, Record};
use br_agentic_contract::{
    AiTurnRef, EntryItem, IncomingContext, InstanceId, MailId, NewMail, RunContext, ThreadRef,
};
use br_graph_contract::{Fields, Reason};
use br_llm_messages::{
    AgentId, Author, BlockId, ContentBlock, Conversation, ConversationId, EntryId, HumanId,
    HumanMessage, IncomingEntry, ModelResponse, OpaqueJson, PlainBlock, RequestId, ResponseEnding,
    ResponseMetadata, RuntimeNotice, RuntimeToolCall, RuntimeToolResult, RuntimeToolResults,
    TaskId, TaskStarted, TextBlock, ToolName, TurnEntry, TurnId, Uuid,
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

pub fn conversation_id() -> ConversationId {
    ConversationId::from_uuid(uuid()).unwrap()
}

pub fn thread_ref() -> ThreadRef {
    ThreadRef::from_uuid(uuid()).unwrap()
}

pub fn instance_id() -> InstanceId {
    InstanceId::new(uuid()).unwrap()
}

pub fn mail_id() -> MailId {
    MailId::new(entry_id())
}

pub fn author(name: &str) -> Author {
    Author::new(name).unwrap()
}

pub fn limit(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

pub fn text_block(text: &str) -> PlainBlock {
    PlainBlock::Text(TextBlock::new(block_id(), text).unwrap())
}

pub fn human(text: &str) -> EntryItem {
    let message = HumanMessage::new(
        entry_id(),
        HumanId::from_uuid(uuid()).unwrap(),
        vec![text_block(text)],
    )
    .unwrap();
    EntryItem::new(IncomingEntry::from(message)).unwrap()
}

pub fn notice(text: &str) -> EntryItem {
    EntryItem::new(IncomingEntry::from(
        RuntimeNotice::new(entry_id(), text).unwrap(),
    ))
    .unwrap()
}

pub fn notification() -> EntryItem {
    let started = TaskStarted::new(
        entry_id(),
        RequestId::from_uuid(uuid()).unwrap(),
        TaskId::from_uuid(uuid()).unwrap(),
    );
    EntryItem::new(IncomingEntry::from(started)).unwrap()
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

pub fn tool_results(entry: EntryId, call: BlockId, name: &str, text: &str) -> RuntimeToolResults {
    let result = RuntimeToolResult::new(
        block_id(),
        call,
        ToolName::new(name).unwrap(),
        false,
        vec![text_block(text)],
    )
    .unwrap();
    RuntimeToolResults::new(entry, vec![result]).unwrap()
}

pub fn new_mail(to: &[AgentId], cc: &[AgentId], subject: &str) -> NewMail {
    NewMail {
        id: mail_id(),
        author: author("sender"),
        source: conversation_id(),
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
    pub record: Record,
    pub thread: ThreadRef,
    pub agent: AgentId,
    pub author: Author,
    pub access: MemoryThreadAccess,
    pub incoming: MemoryIncoming,
}

impl World {
    pub fn new() -> Self {
        Self::with_conversation(Conversation::new(conversation_id()))
    }

    pub fn with_conversation(conversation: Conversation) -> Self {
        let record = Record::new();
        let thread = thread_ref();
        Self {
            access: MemoryThreadAccess::new(&record, thread, conversation),
            incoming: MemoryIncoming::new(&record),
            record,
            thread,
            agent: agent_id(),
            author: author("agent"),
        }
    }

    pub fn context(&self) -> IncomingContext {
        self.context_of(self.agent)
    }

    pub fn context_of(&self, agent: AgentId) -> IncomingContext {
        IncomingContext {
            instance: instance_id(),
            thread: self.thread,
            agent,
        }
    }

    pub async fn open_turn_with_call(&self) -> (AiTurnRef, BlockId) {
        let turn = self.ai(turn_id());
        let call = block_id();
        self.access
            .add_entry(
                &self.thread,
                turn.clone(),
                TurnEntry::from(response_call(entry_id(), call, "lookup")),
            )
            .await
            .unwrap();
        (turn, call)
    }

    pub fn ai(&self, id: TurnId) -> AiTurnRef {
        AiTurnRef {
            id,
            agent: self.agent,
            author: self.author.clone(),
        }
    }
}

pub fn run_context(thread: ThreadRef, agent: AgentId) -> RunContext {
    RunContext::new(
        instance_id(),
        thread,
        agent,
        author("agent"),
        conversation_id(),
    )
}

pub fn fields() -> Doc {
    Doc { n: 1 }
}
