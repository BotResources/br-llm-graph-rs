use br_llm_clients_core::{BoxFuture, StreamSink};
use br_llm_messages::{
    AgentId, Author, BlockId, EntryId, ModelResponse, PlainBlock, ProjectedThread,
    RuntimeToolResult, RuntimeToolResults, TurnId,
};

use crate::fault::{PortFault, ThreadError};
use crate::ids::ThreadRef;
use crate::inbox_entry::InboxEntry;
use crate::mail::MailItem;
use crate::tool::ResultStream;

pub type ResponseStream = Box<dyn StreamSink>;
pub type ResultStreamBox = Box<dyn ResultStream>;

#[derive(Debug, Clone, PartialEq)]
pub enum TurnTarget {
    Input(TurnId),
    Ai { id: TurnId, author: Author },
}

impl TurnTarget {
    pub fn id(&self) -> TurnId {
        match self {
            TurnTarget::Input(id) => *id,
            TurnTarget::Ai { id, .. } => *id,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WrittenEntry {
    pub id: EntryId,
    pub sender: AgentId,
    pub author: Author,
    pub content: Vec<PlainBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Incoming {
    Mail(MailItem),
    Entry(InboxEntry),
    Written(WrittenEntry),
}

impl Incoming {
    pub fn id(&self) -> EntryId {
        match self {
            Incoming::Mail(item) => item.id().entry_id(),
            Incoming::Entry(entry) => entry.id(),
            Incoming::Written(entry) => entry.id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stored {
    Added,
    Held,
}

pub enum Opened<S, H> {
    Stream(S),
    Held(H),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Completed<H> {
    Added,
    Held(H),
}

pub trait ThreadAccess: Send + Sync {
    fn load<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
    ) -> BoxFuture<'a, Result<ProjectedThread, PortFault>>;

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        turn: TurnId,
        items: Vec<Incoming>,
    ) -> BoxFuture<'a, Result<Stored, ThreadError>>;

    fn add_incoming<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        item: Incoming,
    ) -> BoxFuture<'a, Result<Stored, ThreadError>>;

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
    ) -> BoxFuture<'a, Result<Opened<ResponseStream, ModelResponse>, ThreadError>>;

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        response: ModelResponse,
    ) -> BoxFuture<'a, Result<Completed<ModelResponse>, ThreadError>>;

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        entry: EntryId,
    ) -> BoxFuture<'a, Result<(), ThreadError>>;

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
        call: BlockId,
    ) -> BoxFuture<'a, Result<Opened<ResultStreamBox, RuntimeToolResults>, ThreadError>>;

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
        result: RuntimeToolResult,
    ) -> BoxFuture<'a, Result<Completed<RuntimeToolResults>, ThreadError>>;

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        entry: EntryId,
    ) -> BoxFuture<'a, Result<(), ThreadError>>;
}
