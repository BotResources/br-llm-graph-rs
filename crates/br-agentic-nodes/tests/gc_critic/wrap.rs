use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use br_agentic_contract::memory::MemoryThreadAccess;
use br_agentic_contract::{
    AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox, ThreadAccess, ThreadRef,
};
use br_llm_messages::{
    AgentId, Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure,
    TurnEntry,
};

pub enum OnComplete {
    Refuse,
    Hang,
}

pub struct Tripwire {
    inner: Arc<MemoryThreadAccess>,
    agent: AgentId,
    mode: OnComplete,
    hit: Arc<AtomicBool>,
}

impl Tripwire {
    pub fn new(inner: &Arc<MemoryThreadAccess>, agent: AgentId, mode: OnComplete) -> Arc<Self> {
        Arc::new(Self {
            inner: inner.clone(),
            agent,
            mode,
            hit: Arc::default(),
        })
    }

    pub fn hit(&self) -> bool {
        self.hit.load(Ordering::SeqCst)
    }
}

impl ThreadAccess for Tripwire {
    fn read<'a>(
        &'a self,
        thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        self.inner.read(thread)
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.add_input_turn(thread, turn)
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.add_entry(thread, turn, entry)
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.close_turn(thread, turn, closure)
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        self.inner.open_response(thread, turn, entry)
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        if turn.agent == self.agent {
            self.hit.store(true, Ordering::SeqCst);
            return match self.mode {
                OnComplete::Refuse => {
                    Box::pin(async { Err(PortError::fault("the completion is refused")) })
                }
                OnComplete::Hang => Box::pin(std::future::pending()),
            };
        }
        self.inner.complete_response(thread, turn, response)
    }

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.discard_response(thread, entry)
    }

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResultStreamBox, PortError>> {
        self.inner.open_result(thread, turn, entry)
    }

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        result: RuntimeToolResults,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.complete_result(thread, turn, result)
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.discard_result(thread, entry)
    }
}
