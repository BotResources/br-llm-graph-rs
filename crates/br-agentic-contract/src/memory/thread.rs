use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure, TurnEntry,
};

use super::record::{Call, Record};
use super::streams::{MemoryResponseStream, MemoryResultStream};
use crate::error::PortError;
use crate::future::PortFuture;
use crate::ids::ThreadRef;
use crate::rules::{
    SessionError, apply_closure, apply_entry, apply_input_turn, apply_response, apply_results,
};
use crate::stream::{ResponseStream, ResultStreamBox};
use crate::thread_access::{AiTurnRef, ThreadAccess};

struct State {
    conversation: Conversation,
    open: HashMap<EntryId, Arc<AtomicBool>>,
}

#[derive(Clone)]
pub struct MemoryThreadAccess {
    thread: ThreadRef,
    record: Record,
    state: Arc<Mutex<State>>,
}

impl MemoryThreadAccess {
    pub fn new(record: &Record, thread: ThreadRef, conversation: Conversation) -> Self {
        Self {
            thread,
            record: record.clone(),
            state: Arc::new(Mutex::new(State {
                conversation,
                open: HashMap::new(),
            })),
        }
    }

    pub fn conversation(&self) -> Conversation {
        self.lock().conversation.clone()
    }

    pub fn is_open(&self, entry: EntryId) -> bool {
        self.lock().open.contains_key(&entry)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn check(&self, thread: &ThreadRef) -> Result<(), PortError> {
        if *thread == self.thread {
            Ok(())
        } else {
            Err(PortError::fault(format!("unknown thread {thread}")))
        }
    }

    fn store(
        &self,
        thread: &ThreadRef,
        apply: impl FnOnce(&mut Conversation) -> Result<(), SessionError>,
    ) -> Result<(), PortError> {
        self.check(thread)?;
        apply(&mut self.lock().conversation).map_err(PortError::fault)
    }

    fn begin(&self, entry: EntryId) -> Arc<AtomicBool> {
        let alive = Arc::new(AtomicBool::new(true));
        if let Some(previous) = self.lock().open.insert(entry, alive.clone()) {
            previous.store(false, Ordering::SeqCst);
        }
        alive
    }

    fn end(&self, entry: EntryId) {
        if let Some(alive) = self.lock().open.remove(&entry) {
            alive.store(false, Ordering::SeqCst);
        }
    }
}

impl ThreadAccess for MemoryThreadAccess {
    fn read<'a>(
        &'a self,
        thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        self.record.push(Call::Read { thread: *thread });
        Box::pin(async move {
            self.check(thread)?;
            Ok(self.conversation())
        })
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::AddInputTurn {
            thread: *thread,
            turn: turn.clone(),
        });
        Box::pin(async move { self.store(thread, |c| apply_input_turn(c, &turn)) })
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::AddEntry {
            thread: *thread,
            turn: turn.clone(),
            entry: entry.clone(),
        });
        Box::pin(async move { self.store(thread, |c| apply_entry(c, &turn, entry)) })
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::CloseTurn {
            thread: *thread,
            turn: turn.clone(),
            closure: closure.clone(),
        });
        Box::pin(async move { self.store(thread, |c| apply_closure(c, &turn, closure)) })
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        self.record.push(Call::OpenResponse {
            thread: *thread,
            turn,
            entry,
        });
        Box::pin(async move {
            self.check(thread)?;
            let alive = self.begin(entry);
            let stream: ResponseStream = Box::new(MemoryResponseStream {
                entry,
                alive,
                record: self.record.clone(),
            });
            Ok(stream)
        })
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::CompleteResponse {
            thread: *thread,
            turn: turn.clone(),
            response: response.clone(),
        });
        Box::pin(async move {
            let entry = response.id();
            self.store(thread, |c| apply_response(c, &turn, response))?;
            self.end(entry);
            Ok(())
        })
    }

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::DiscardResponse {
            thread: *thread,
            entry,
        });
        Box::pin(async move {
            self.check(thread)?;
            self.end(entry);
            Ok(())
        })
    }

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResultStreamBox, PortError>> {
        self.record.push(Call::OpenResult {
            thread: *thread,
            turn,
            entry,
        });
        Box::pin(async move {
            self.check(thread)?;
            let alive = self.begin(entry);
            let stream: ResultStreamBox = Box::new(MemoryResultStream {
                entry,
                alive,
                record: self.record.clone(),
            });
            Ok(stream)
        })
    }

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        result: RuntimeToolResults,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::CompleteResult {
            thread: *thread,
            turn: turn.clone(),
            result: result.clone(),
        });
        Box::pin(async move {
            let entry = result.id();
            self.store(thread, |c| apply_results(c, &turn, result))?;
            self.end(entry);
            Ok(())
        })
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(Call::DiscardResult {
            thread: *thread,
            entry,
        });
        Box::pin(async move {
            self.check(thread)?;
            self.end(entry);
            Ok(())
        })
    }
}
