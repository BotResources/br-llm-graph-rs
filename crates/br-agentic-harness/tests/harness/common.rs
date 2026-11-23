use std::error::Error;
use std::sync::Arc;

use br_agentic_contract::memory::{FixedToolsPort, MemoryIncoming, MemoryThreadAccess, Record};
use br_agentic_contract::{
    AiTurnRef, Fields, IncomingContext, InstanceId, MailId, NewMail, RunContext, ThreadAccess,
    ThreadRef, Tool, ToolBlock, ToolContent,
};
use br_agentic_harness::{Harness, HarnessBuilder, HarnessHistory, MailPort, SandboxPort};
use br_agentic_harness_contract::memory::{
    HarnessRecord, MemoryHarnessHistory, MemoryMail, MemorySandbox, MemorySkills,
};
use br_graph_contract::{CancelToken, Reason};
use br_llm_messages::{
    AgentId, AgentMail, Author, BlockId, Conversation, ConversationId, EntryId, IncomingEntry,
    InputTurn, OpaqueJson, PlainBlock, TextBlock, TurnId, Uuid,
};
use serde::{Deserialize, Serialize};

pub use crate::doubles::{FaultyTools, FixedPrompt, NullSink, runner_tool};

pub type Res = Result<(), Box<dyn Error>>;

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

pub fn uuid() -> Uuid {
    Uuid::now_v7()
}

pub fn entry_id() -> Result<EntryId, Box<dyn Error>> {
    Ok(EntryId::from_uuid(uuid())?)
}

pub fn agent_id() -> Result<AgentId, Box<dyn Error>> {
    Ok(AgentId::from_uuid(uuid())?)
}

pub fn instance_id() -> Result<InstanceId, Box<dyn Error>> {
    Ok(InstanceId::new(uuid())?)
}

pub fn thread_ref() -> Result<ThreadRef, Box<dyn Error>> {
    Ok(ThreadRef::from_uuid(uuid())?)
}

pub fn conversation_id() -> Result<ConversationId, Box<dyn Error>> {
    Ok(ConversationId::from_uuid(uuid())?)
}

pub fn limit(n: usize) -> Result<std::num::NonZeroUsize, Box<dyn Error>> {
    std::num::NonZeroUsize::new(n).ok_or_else(|| "a limit is not zero".into())
}

pub async fn settle() {
    for _ in 0..64 {
        tokio::task::yield_now().await;
    }
}

pub struct World {
    pub record: HarnessRecord,
    pub contract_record: Record,
    pub thread: ThreadRef,
    pub agent: AgentId,
    pub author: Author,
    pub conversation: ConversationId,
    pub instance: InstanceId,
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
            contract_record,
            thread,
            agent: agent_id()?,
            author: Author::new("agent")?,
            conversation: conversation_id()?,
            instance: instance_id()?,
        })
    }

    pub fn builder(&self) -> HarnessBuilder<Doc> {
        self.builder_with(Vec::new())
    }

    pub fn builder_with(&self, tools: Vec<Tool>) -> HarnessBuilder<Doc> {
        Harness::builder(
            Arc::new(self.history.clone()) as Arc<dyn HarnessHistory>,
            Arc::new(FixedPrompt("the prompt of the runner".to_owned())),
            Arc::new(FixedToolsPort::new(&self.contract_record, tools)),
        )
    }

    pub fn full(&self) -> HarnessBuilder<Doc> {
        self.builder()
            .mail(Arc::new(self.mail.clone()) as Arc<dyn MailPort>)
            .sandbox(Arc::new(self.sandbox.clone()) as Arc<dyn SandboxPort>)
            .skills(Arc::new(MemorySkills::new(&self.record)))
    }

    pub fn run_context(&self) -> RunContext {
        RunContext::new(
            self.instance,
            self.thread,
            self.agent,
            self.author.clone(),
            self.conversation,
        )
    }

    pub fn context(&self) -> IncomingContext {
        IncomingContext {
            instance: self.instance,
            thread: self.thread,
            agent: self.agent,
        }
    }

    pub fn turn(&self) -> Result<AiTurnRef, Box<dyn Error>> {
        Ok(AiTurnRef {
            id: TurnId::from_uuid(uuid())?,
            agent: self.agent,
            author: self.author.clone(),
        })
    }

    pub fn deliver(&self, mail: NewMail) -> Res {
        self.mail.incoming().deliver_mail(agent_id()?, mail)?;
        Ok(())
    }
}

pub fn new_mail(to: &[AgentId], subject: &str) -> Result<NewMail, Box<dyn Error>> {
    Ok(NewMail {
        id: MailId::new(entry_id()?),
        author: Author::new("sender")?,
        source: conversation_id()?,
        to: to.to_vec(),
        cc: Vec::new(),
        subject: subject.to_owned(),
        body: format!("body of {subject}"),
        attachments: Vec::new(),
    })
}

pub fn mail_entry(id: EntryId) -> Result<AgentMail, Box<dyn Error>> {
    let text = TextBlock::new(BlockId::from_uuid(uuid())?, "a mail")?;
    Ok(AgentMail::new(
        id,
        agent_id()?,
        Author::new("sender")?,
        conversation_id()?,
        vec![PlainBlock::Text(text)],
    )?)
}

pub fn mail_turn(id: EntryId) -> Result<InputTurn, Box<dyn Error>> {
    Ok(InputTurn::new(
        TurnId::from_uuid(uuid())?,
        vec![IncomingEntry::AgentMail(mail_entry(id)?)],
    )?)
}

pub async fn store_mail(history: &dyn ThreadAccess, thread: &ThreadRef, id: EntryId) -> Res {
    history.add_input_turn(thread, mail_turn(id)?).await?;
    Ok(())
}

pub fn text_of(content: &ToolContent) -> String {
    content
        .content()
        .iter()
        .filter_map(|block| match block {
            ToolBlock::Text(text) => Some(text.as_str().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub async fn call(tool: &Tool, json: &str) -> Result<ToolContent, Box<dyn Error>> {
    let arguments = OpaqueJson::parse(json)?;
    Ok(tool
        .call(&arguments, CancelToken::new().view(), &mut NullSink)
        .await?)
}

pub async fn offered(harness: &Harness<Doc>, world: &World) -> Result<Vec<Tool>, Box<dyn Error>> {
    Ok(harness
        .tools()
        .tools(&Doc { n: 0 }, &world.run_context())
        .await?)
}

pub fn find(tools: &[Tool], name: &str) -> Result<Tool, Box<dyn Error>> {
    tools
        .iter()
        .find(|tool| tool.name().as_str() == name)
        .cloned()
        .ok_or_else(|| format!("the tool {name} is not offered").into())
}

pub fn response_entry(id: EntryId) -> Result<br_llm_messages::TurnEntry, Box<dyn Error>> {
    use br_llm_messages::{ContentBlock, ModelResponse, ResponseEnding, ResponseMetadata};
    Ok(br_llm_messages::TurnEntry::from(ModelResponse::new(
        id,
        vec![ContentBlock::Text(TextBlock::new(
            BlockId::from_uuid(uuid())?,
            "stored",
        )?)],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )?))
}

impl World {
    pub fn mail_documents_asked(&self) -> bool {
        self.record.calls().iter().any(|call| {
            matches!(
                call,
                br_agentic_harness_contract::memory::HarnessCall::MailDocuments { .. }
            )
        })
    }
}

impl World {
    pub fn full_with(&self, tools: Vec<Tool>) -> HarnessBuilder<Doc> {
        self.builder_with(tools)
            .mail(Arc::new(self.mail.clone()) as Arc<dyn MailPort>)
            .sandbox(Arc::new(self.sandbox.clone()) as Arc<dyn SandboxPort>)
    }
}
