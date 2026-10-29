use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use br_graph_contract::Reason;
use br_llm_clients_core::{BoxFuture, StreamSink};
use br_llm_messages::{
    AgentId, BlockId, Conversation, EntryId, EntryRef, ModelResponse, ResponseDelta,
    RuntimeToolResult, RuntimeToolResults, Turn, TurnEntry,
};

use super::additions::{append, holds_entry};
use super::build::refused;
use super::state::{MemoryHub, OpenObject};
use crate::fault::ThreadError;
use crate::ids::ThreadRef;
use crate::thread_access::{Completed, Opened, ResponseStream, ResultStreamBox, TurnTarget};
use crate::tool::ResultStream;

struct MemoryResponseStream {
    alive: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<ResponseDelta>>>,
}

impl StreamSink for MemoryResponseStream {
    fn send<'a>(&'a mut self, delta: ResponseDelta) -> BoxFuture<'a, ()> {
        if self.alive.load(Ordering::SeqCst) {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(delta);
        }
        Box::pin(async {})
    }
}

struct MemoryResultStream {
    alive: Arc<AtomicBool>,
    log: Arc<Mutex<Vec<String>>>,
}

impl ResultStream for MemoryResultStream {
    fn send<'a>(&'a mut self, fragment: String) -> BoxFuture<'a, ()> {
        if self.alive.load(Ordering::SeqCst) {
            self.log
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(fragment);
        }
        Box::pin(async {})
    }
}

fn held_response(conversation: &Conversation, id: EntryId) -> Option<ModelResponse> {
    match conversation.entry(&id) {
        Ok(EntryRef::Turn(TurnEntry::ModelResponse(response))) => Some(response.clone()),
        _ => None,
    }
}

fn held_results(conversation: &Conversation, id: EntryId) -> Option<RuntimeToolResults> {
    match conversation.entry(&id) {
        Ok(EntryRef::Turn(TurnEntry::RuntimeToolResults(results))) => Some(results.clone()),
        _ => None,
    }
}

fn wrong_object(id: EntryId) -> ThreadError {
    ThreadError::Refused(Reason::new(format!(
        "the thread holds {} as another kind of entry",
        id.as_uuid()
    )))
}

fn refuse(text: &str) -> ThreadError {
    ThreadError::Refused(Reason::new(text))
}

fn check_open_state(conversation: &Conversation, target: &TurnTarget) -> Result<(), ThreadError> {
    match conversation.turn(&target.id()) {
        Ok(Turn::AITurn(ai)) if !ai.is_closed() => Ok(()),
        Ok(Turn::AITurn(_)) => Err(refuse("the AI turn is closed")),
        Ok(Turn::InputTurn(_)) => Err(refuse("the turn is an input turn")),
        Err(_) => match target {
            TurnTarget::Ai { .. } => Ok(()),
            TurnTarget::Input(_) => Err(refuse("the turn does not exist")),
        },
        Ok(_) => Err(refuse("the turn is not an AI turn")),
    }
}

impl MemoryHub {
    fn begin_open(&self, entry: EntryId) -> Arc<AtomicBool> {
        let mut state = self.lock();
        if let Some(previous) = state.open.remove(&entry) {
            previous.alive.store(false, Ordering::SeqCst);
        }
        let alive = Arc::new(AtomicBool::new(true));
        state.open.insert(
            entry,
            OpenObject {
                alive: alive.clone(),
            },
        );
        alive
    }

    fn end_open(&self, entry: EntryId) {
        if let Some(open) = self.lock().open.remove(&entry) {
            open.alive.store(false, Ordering::SeqCst);
        }
    }

    pub(super) fn open_response(
        &self,
        thread: &ThreadRef,
        target: &TurnTarget,
        entry: EntryId,
    ) -> Result<Opened<ResponseStream, ModelResponse>, ThreadError> {
        let conversation = self.lock().conversation(thread)?.clone();
        if holds_entry(&conversation, entry) {
            return held_response(&conversation, entry)
                .map(Opened::Held)
                .ok_or_else(|| wrong_object(entry));
        }
        check_open_state(&conversation, target)?;
        let alive = self.begin_open(entry);
        let log = Arc::new(Mutex::new(Vec::new()));
        self.lock().deltas.insert(entry, log.clone());
        Ok(Opened::Stream(Box::new(MemoryResponseStream {
            alive,
            log,
        })))
    }

    pub(super) fn complete_response(
        &self,
        thread: &ThreadRef,
        agent: AgentId,
        target: &TurnTarget,
        response: ModelResponse,
    ) -> Result<Completed<ModelResponse>, ThreadError> {
        let entry = response.id();
        let mut state = self.lock();
        let conversation = state.conversation(thread)?;
        if holds_entry(conversation, entry) {
            return held_response(conversation, entry)
                .map(Completed::Held)
                .ok_or_else(|| wrong_object(entry));
        }
        let mut next = conversation.clone();
        append(&mut next, agent, target, TurnEntry::from(response))?;
        *state.conversation_mut(thread)? = next;
        drop(state);
        self.end_open(entry);
        Ok(Completed::Added)
    }

    pub(super) fn discard(&self, entry: EntryId) {
        self.end_open(entry);
    }

    pub(super) fn open_result(
        &self,
        thread: &ThreadRef,
        target: &TurnTarget,
        entry: EntryId,
        call: BlockId,
    ) -> Result<Opened<ResultStreamBox, RuntimeToolResults>, ThreadError> {
        let conversation = self.lock().conversation(thread)?.clone();
        if holds_entry(&conversation, entry) {
            return held_results(&conversation, entry)
                .map(Opened::Held)
                .ok_or_else(|| wrong_object(entry));
        }
        match conversation.turn(&target.id()) {
            Ok(Turn::AITurn(ai))
                if !ai.is_closed() && ai.state().outstanding_runtime_calls().contains(&call) => {}
            _ => return Err(refuse("the tool call is not pending")),
        }
        let alive = self.begin_open(entry);
        let log = Arc::new(Mutex::new(Vec::new()));
        self.lock().fragments.insert(entry, log.clone());
        Ok(Opened::Stream(Box::new(MemoryResultStream { alive, log })))
    }

    pub(super) fn complete_result(
        &self,
        thread: &ThreadRef,
        agent: AgentId,
        target: &TurnTarget,
        entry: EntryId,
        result: RuntimeToolResult,
    ) -> Result<Completed<RuntimeToolResults>, ThreadError> {
        let mut state = self.lock();
        let conversation = state.conversation(thread)?;
        if holds_entry(conversation, entry) {
            return held_results(conversation, entry)
                .map(Completed::Held)
                .ok_or_else(|| wrong_object(entry));
        }
        let results = RuntimeToolResults::new(entry, vec![result]).map_err(refused)?;
        let mut next = conversation.clone();
        append(&mut next, agent, target, TurnEntry::from(results))?;
        *state.conversation_mut(thread)? = next;
        drop(state);
        self.end_open(entry);
        Ok(Completed::Added)
    }
}
