use br_llm_clients_core::BoxFuture;
use br_llm_messages::{
    AgentId, BlockId, EntryId, ModelResponse, ProjectedThread, RuntimeToolResult,
    RuntimeToolResults, TurnClosure, TurnId,
};

use super::state::MemoryHub;
use crate::fault::{PortFault, ThreadError};
use crate::ids::ThreadRef;
use crate::thread_access::{
    Completed, Incoming, Opened, ResponseStream, ResultStreamBox, Stored, ThreadAccess, TurnTarget,
};

#[derive(Clone)]
pub struct MemoryThreadAccess {
    hub: MemoryHub,
}

impl MemoryHub {
    pub fn thread_access(&self) -> MemoryThreadAccess {
        MemoryThreadAccess { hub: self.clone() }
    }
}

impl ThreadAccess for MemoryThreadAccess {
    fn load<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
    ) -> BoxFuture<'a, Result<ProjectedThread, PortFault>> {
        Box::pin(async move {
            let state = self.hub.lock();
            state
                .threads
                .get(thread)
                .map(|conversation| conversation.project_for(agent))
                .ok_or_else(|| PortFault::new(format!("unknown thread {thread}")))
        })
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        _agent: AgentId,
        turn: TurnId,
        items: Vec<Incoming>,
    ) -> BoxFuture<'a, Result<Stored, ThreadError>> {
        Box::pin(async move { self.hub.add_input_turn(thread, turn, &items) })
    }

    fn add_incoming<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        item: Incoming,
    ) -> BoxFuture<'a, Result<Stored, ThreadError>> {
        Box::pin(async move { self.hub.add_incoming(thread, agent, &target, &item) })
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        _agent: AgentId,
        turn: TurnId,
        closure: TurnClosure,
    ) -> BoxFuture<'a, Result<Stored, ThreadError>> {
        Box::pin(async move { self.hub.close_turn(thread, turn, closure) })
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        _agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
    ) -> BoxFuture<'a, Result<Opened<ResponseStream, ModelResponse>, ThreadError>> {
        Box::pin(async move { self.hub.open_response(thread, &target, entry) })
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        response: ModelResponse,
    ) -> BoxFuture<'a, Result<Completed<ModelResponse>, ThreadError>> {
        Box::pin(async move { self.hub.complete_response(thread, agent, &target, response) })
    }

    fn discard_response<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _agent: AgentId,
        entry: EntryId,
    ) -> BoxFuture<'a, Result<(), ThreadError>> {
        Box::pin(async move {
            self.hub.discard(entry);
            Ok(())
        })
    }

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        _agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
        call: BlockId,
    ) -> BoxFuture<'a, Result<Opened<ResultStreamBox, RuntimeToolResults>, ThreadError>> {
        Box::pin(async move { self.hub.open_result(thread, &target, entry, call) })
    }

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        agent: AgentId,
        target: TurnTarget,
        entry: EntryId,
        result: RuntimeToolResult,
    ) -> BoxFuture<'a, Result<Completed<RuntimeToolResults>, ThreadError>> {
        Box::pin(async move {
            self.hub
                .complete_result(thread, agent, &target, entry, result)
        })
    }

    fn discard_result<'a>(
        &'a self,
        _thread: &'a ThreadRef,
        _agent: AgentId,
        entry: EntryId,
    ) -> BoxFuture<'a, Result<(), ThreadError>> {
        Box::pin(async move {
            self.hub.discard(entry);
            Ok(())
        })
    }
}
