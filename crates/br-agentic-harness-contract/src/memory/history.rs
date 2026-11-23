use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::Poll;

use br_agentic_contract::memory::MemoryThreadAccess;
use br_agentic_contract::{AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox};
use br_agentic_contract::{ThreadAccess, ThreadRef};
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure, TurnEntry,
};

use super::record::{HarnessCall, HarnessRecord};
use super::wakers::Wakers;
use crate::history::{HarnessHistory, StoredAnswer};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Pending,
    Dropped,
}

#[derive(Default)]
struct State {
    slots: HashMap<EntryId, Slot>,
    channel_closed: bool,
}

struct Shared {
    state: Mutex<State>,
    wakers: Wakers,
}

#[derive(Clone)]
pub struct MemoryHarnessHistory {
    thread: ThreadRef,
    record: HarnessRecord,
    access: MemoryThreadAccess,
    shared: Arc<Shared>,
}

impl MemoryHarnessHistory {
    pub fn new(record: &HarnessRecord, thread: ThreadRef, access: MemoryThreadAccess) -> Self {
        Self {
            thread,
            record: record.clone(),
            access,
            shared: Arc::new(Shared {
                state: Mutex::new(State::default()),
                wakers: Wakers::default(),
            }),
        }
    }

    pub fn conversation(&self) -> Conversation {
        self.access.conversation()
    }

    pub fn expect(&self, entry: EntryId) {
        self.lock().slots.insert(entry, Slot::Pending);
    }

    pub fn drop_entry(&self, entry: EntryId) {
        self.lock().slots.insert(entry, Slot::Dropped);
        self.shared.wakers.wake_all();
    }

    pub fn close_channel(&self) {
        self.lock().channel_closed = true;
        self.shared.wakers.wake_all();
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn answer(&self, entry: EntryId) -> Option<Result<StoredAnswer, PortError>> {
        let (slot, closed) = {
            let state = self.lock();
            (state.slots.get(&entry).copied(), state.channel_closed)
        };
        if slot == Some(Slot::Dropped) {
            return Some(Ok(StoredAnswer::Dropped));
        }
        if self.access.conversation().entry(&entry).is_ok() {
            return Some(Ok(StoredAnswer::Stored));
        }
        match slot {
            None => Some(Err(PortError::fault(
                "the history holds no record of that entry",
            ))),
            Some(_) if closed => Some(Err(PortError::fault(
                "the change channel closed before the entry was stored",
            ))),
            Some(_) => None,
        }
    }

    async fn written<T>(
        &self,
        write: impl Future<Output = Result<T, PortError>>,
    ) -> Result<T, PortError> {
        let result = write.await;
        self.shared.wakers.wake_all();
        result
    }
}

impl HarnessHistory for MemoryHarnessHistory {
    fn stored<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<StoredAnswer, PortError>> {
        self.record.push(HarnessCall::Stored {
            thread: *thread,
            entry,
        });
        Box::pin(async move {
            if *thread != self.thread {
                return Err(PortError::fault(format!(
                    "the history has no slot for {thread}"
                )));
            }
            std::future::poll_fn(|cx| {
                self.shared.wakers.register(cx.waker());
                match self.answer(entry) {
                    Some(answer) => Poll::Ready(answer),
                    None => Poll::Pending,
                }
            })
            .await
        })
    }
}

impl ThreadAccess for MemoryHarnessHistory {
    fn read<'a>(
        &'a self,
        thread: &'a ThreadRef,
    ) -> PortFuture<'a, Result<Conversation, PortError>> {
        self.access.read(thread)
    }

    fn add_input_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: InputTurn,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(self.written(self.access.add_input_turn(thread, turn)))
    }

    fn add_entry<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(self.written(self.access.add_entry(thread, turn, entry)))
    }

    fn close_turn<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(self.written(self.access.close_turn(thread, turn, closure)))
    }

    fn open_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResponseStream, PortError>> {
        self.access.open_response(thread, turn, entry)
    }

    fn complete_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(self.written(self.access.complete_response(thread, turn, response)))
    }

    fn discard_response<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.access.discard_response(thread, entry)
    }

    fn open_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<ResultStreamBox, PortError>> {
        self.access.open_result(thread, turn, entry)
    }

    fn complete_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        turn: AiTurnRef,
        result: RuntimeToolResults,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(self.written(self.access.complete_result(thread, turn, result)))
    }

    fn discard_result<'a>(
        &'a self,
        thread: &'a ThreadRef,
        entry: EntryId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.access.discard_result(thread, entry)
    }
}
