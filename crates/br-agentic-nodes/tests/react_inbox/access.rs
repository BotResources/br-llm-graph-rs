use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_contract::memory::MemoryThreadAccess;
use br_agentic_contract::{
    AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox, ThreadAccess, ThreadRef,
};
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure, TurnEntry,
};

pub struct FailingAdds {
    inner: Arc<MemoryThreadAccess>,
    input_turns: AtomicUsize,
    entries: AtomicUsize,
    fail_input_turn: Option<usize>,
    fail_entry: Option<usize>,
}

impl FailingAdds {
    pub fn input_turn(inner: &Arc<MemoryThreadAccess>, nth: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: inner.clone(),
            input_turns: AtomicUsize::new(0),
            entries: AtomicUsize::new(0),
            fail_input_turn: Some(nth),
            fail_entry: None,
        })
    }

    pub fn entry(inner: &Arc<MemoryThreadAccess>, nth: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: inner.clone(),
            input_turns: AtomicUsize::new(0),
            entries: AtomicUsize::new(0),
            fail_input_turn: None,
            fail_entry: Some(nth),
        })
    }
}

impl ThreadAccess for FailingAdds {
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
        let nth = self.input_turns.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_input_turn == Some(nth) {
            return Box::pin(async { Err(PortError::fault("the thread access refused the turn")) });
        }
        self.inner.add_input_turn(thread, turn)
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        let nth = self.entries.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_entry == Some(nth) {
            return Box::pin(async {
                Err(PortError::fault("the thread access refused the entry"))
            });
        }
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
