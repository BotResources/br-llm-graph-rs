use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use br_agentic_contract::{
    AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox, ThreadAccess, ThreadRef,
};
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure, TurnEntry,
};

pub struct Flaky<A: ThreadAccess> {
    pub inner: A,
    pub fail_adds: AtomicBool,
    pub fail_discards: AtomicBool,
    pub fail_reads: AtomicBool,
    pub refuse_adds: AtomicBool,
    pub ok_adds: AtomicUsize,
    pub failed_calls: AtomicUsize,
}

impl<A: ThreadAccess> Flaky<A> {
    pub fn new(inner: A) -> Self {
        Self {
            inner,
            fail_adds: AtomicBool::new(false),
            fail_discards: AtomicBool::new(false),
            fail_reads: AtomicBool::new(false),
            refuse_adds: AtomicBool::new(false),
            ok_adds: AtomicUsize::new(0),
            failed_calls: AtomicUsize::new(0),
        }
    }

    fn fails(&self, flag: &AtomicBool) -> Option<PortError> {
        if std::ptr::eq(flag, &self.fail_adds) && self.refuse_adds.load(Ordering::SeqCst) {
            return Some(PortError::refused("refused by the thread access"));
        }
        if std::ptr::eq(flag, &self.fail_adds)
            && flag.load(Ordering::SeqCst)
            && self
                .ok_adds
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
        {
            return None;
        }
        flag.load(Ordering::SeqCst).then(|| {
            self.failed_calls.fetch_add(1, Ordering::SeqCst);
            PortError::fault("injected failure")
        })
    }
}

impl<A: ThreadAccess> ThreadAccess for Flaky<A> {
    fn read<'a>(
        &'a self,
        thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        match self.fails(&self.fail_reads) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.read(thread),
        }
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        match self.fails(&self.fail_adds) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.add_input_turn(thread, turn),
        }
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        match self.fails(&self.fail_adds) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.add_entry(thread, turn, entry),
        }
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        match self.fails(&self.fail_adds) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.close_turn(thread, turn, closure),
        }
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
        match self.fails(&self.fail_adds) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.complete_response(thread, turn, response),
        }
    }

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        match self.fails(&self.fail_discards) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.discard_response(thread, entry),
        }
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
        match self.fails(&self.fail_adds) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.complete_result(thread, turn, result),
        }
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        match self.fails(&self.fail_discards) {
            Some(error) => Box::pin(async move { Err(error) }),
            None => self.inner.discard_result(thread, entry),
        }
    }
}
