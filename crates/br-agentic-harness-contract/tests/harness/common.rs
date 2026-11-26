use std::error::Error;
use std::num::NonZeroUsize;

use br_agentic_contract::memory::{MemoryIncoming, MemoryThreadAccess, Record};
use br_agentic_contract::{
    AiTurnRef, IncomingContext, InstanceId, MailId, NewMail, ThreadRef, WebAccess,
};
use br_agentic_harness_contract::memory::{
    HarnessRecord, MemoryHarnessHistory, MemoryMail, MemorySandbox,
};
use br_agentic_harness_contract::{BoxId, OpenRequest, SandboxPort, Seed, TurnRef};
use br_llm_messages::{
    AgentId, Author, BlockId, ContentBlock, Conversation, ConversationId, EntryId, ModelResponse,
    ResponseEnding, ResponseMetadata, TextBlock, TurnEntry, TurnId, Uuid,
};

pub type Res = Result<(), Box<dyn Error>>;

pub fn uuid() -> Uuid {
    Uuid::now_v7()
}

pub fn entry_id() -> Result<EntryId, Box<dyn Error>> {
    Ok(EntryId::from_uuid(uuid())?)
}

pub fn agent_id() -> Result<AgentId, Box<dyn Error>> {
    Ok(AgentId::from_uuid(uuid())?)
}

pub fn thread_ref() -> Result<ThreadRef, Box<dyn Error>> {
    Ok(ThreadRef::from_uuid(uuid())?)
}

pub fn turn_id() -> Result<TurnId, Box<dyn Error>> {
    Ok(TurnId::from_uuid(uuid())?)
}

pub fn instance_id() -> Result<InstanceId, Box<dyn Error>> {
    Ok(InstanceId::new(uuid())?)
}

pub fn conversation_id() -> Result<ConversationId, Box<dyn Error>> {
    Ok(ConversationId::from_uuid(uuid())?)
}

pub fn mail_id() -> Result<MailId, Box<dyn Error>> {
    Ok(MailId::new(entry_id()?))
}

pub fn limit(n: usize) -> Result<NonZeroUsize, Box<dyn Error>> {
    NonZeroUsize::new(n).ok_or_else(|| "a limit is not zero".into())
}

pub fn new_mail(to: &[AgentId], cc: &[AgentId], subject: &str) -> Result<NewMail, Box<dyn Error>> {
    Ok(NewMail {
        id: mail_id()?,
        author: Author::new("sender")?,
        source: conversation_id()?,
        to: to.to_vec(),
        cc: cc.to_vec(),
        subject: subject.to_owned(),
        body: format!("body of {subject}"),
        attachments: Vec::new(),
    })
}

pub fn response(entry: EntryId, text: &str) -> Result<ModelResponse, Box<dyn Error>> {
    Ok(ModelResponse::new(
        entry,
        vec![ContentBlock::Text(TextBlock::new(
            BlockId::from_uuid(uuid())?,
            text,
        )?)],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )?)
}

pub struct World {
    pub record: HarnessRecord,
    pub thread: ThreadRef,
    pub agent: AgentId,
    pub author: Author,
    pub history: MemoryHarnessHistory,
    pub mail: MemoryMail,
    pub sandbox: MemorySandbox,
}

impl World {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let contract_record = Record::new();
        let record = HarnessRecord::new();
        let thread = thread_ref()?;
        let access = MemoryThreadAccess::new(
            &contract_record,
            thread,
            Conversation::new(conversation_id()?),
        );
        Ok(Self {
            history: MemoryHarnessHistory::new(&record, thread, access),
            mail: MemoryMail::new(&record, MemoryIncoming::new(&contract_record)),
            sandbox: MemorySandbox::new(&record),
            record,
            thread,
            agent: agent_id()?,
            author: Author::new("agent")?,
        })
    }

    pub fn context(&self) -> Result<IncomingContext, Box<dyn Error>> {
        self.context_of(self.agent)
    }

    pub fn context_of(&self, agent: AgentId) -> Result<IncomingContext, Box<dyn Error>> {
        Ok(IncomingContext {
            instance: instance_id()?,
            thread: self.thread,
            agent,
        })
    }

    pub fn turn(&self) -> Result<AiTurnRef, Box<dyn Error>> {
        Ok(AiTurnRef {
            id: TurnId::from_uuid(uuid())?,
            agent: self.agent,
            author: self.author.clone(),
        })
    }

    pub fn response_entry(&self, entry: EntryId) -> Result<TurnEntry, Box<dyn Error>> {
        Ok(TurnEntry::from(response(entry, "stored")?))
    }

    pub fn turn_ref(&self, parent: Option<TurnId>) -> Result<TurnRef, Box<dyn Error>> {
        Ok(TurnRef::new(self.thread, self.agent, turn_id()?, parent))
    }

    pub fn request(&self, web: Option<WebAccess>) -> Result<OpenRequest, Box<dyn Error>> {
        Ok(OpenRequest::new(
            instance_id()?,
            web,
            self.turn_ref(None)?,
            Seed::empty(),
        ))
    }

    pub async fn open_box(&self) -> Result<BoxId, Box<dyn Error>> {
        self.open_with(None).await
    }

    pub async fn open_with(&self, web: Option<WebAccess>) -> Result<BoxId, Box<dyn Error>> {
        let request = self.request(web)?;
        Ok(self.sandbox.open(&request).await?.box_id)
    }
}
