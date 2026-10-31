use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use br_agentic_contract::memory::{FixedThreadPort, FixedToolsPort};
use br_agentic_contract::{
    AgentLabel, AiTurnRef, PortError, PortFuture, ResponseStream, ResultStreamBox, ThreadAccess,
    ThreadRef,
};
use br_agentic_nodes::{AuthorPort, GeneratorCritic, Ports, PromptPort};
use br_llm_messages::{
    Conversation, EntryId, InputTurn, ModelResponse, RuntimeToolResults, TurnClosure, TurnEntry,
};

use crate::common::*;

pub struct BlankPromptFor(pub &'static str);

impl PromptPort<Doc> for BlankPromptFor {
    fn prompt<'a>(
        &'a self,
        _snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move {
            if label.as_str() == self.0 {
                Ok("  ".to_owned())
            } else {
                Ok(format!("system prompt of the {label}"))
            }
        })
    }
}

#[derive(Default)]
pub struct Authors {
    asked: Mutex<HashMap<String, usize>>,
    blank_for: Option<&'static str>,
}

impl Authors {
    pub fn blank_for(label: &'static str) -> Self {
        Self {
            asked: Mutex::default(),
            blank_for: Some(label),
        }
    }

    pub fn asked(&self, label: &str) -> usize {
        self.asked.lock().unwrap().get(label).copied().unwrap_or(0)
    }
}

impl AuthorPort<Doc> for Authors {
    fn author<'a>(
        &'a self,
        _snapshot: &'a Doc,
        label: &'a AgentLabel,
    ) -> PortFuture<'a, Result<String, PortError>> {
        Box::pin(async move {
            *self
                .asked
                .lock()
                .unwrap()
                .entry(label.as_str().to_owned())
                .or_default() += 1;
            if self.blank_for == Some(label.as_str()) {
                return Ok(String::new());
            }
            Ok(format!("Author of the {label}"))
        })
    }
}

pub fn ports_over(
    rig: &Rig,
    access: Arc<dyn ThreadAccess>,
    prompt: Arc<dyn PromptPort<Doc>>,
) -> Ports<Doc> {
    Ports::new(
        Arc::new(FixedThreadPort::new(&rig.record, rig.thread)),
        rig.agents.clone(),
        access,
        prompt,
        Arc::new(FixedToolsPort::new(&rig.record, rig.tools.clone())),
        rig.workspace.clone(),
    )
}

pub fn pair_over(
    rig: &Rig,
    ports: Ports<Doc>,
    spec: br_agentic_nodes::GeneratorCriticSpec<Doc>,
) -> GeneratorCritic<Doc> {
    GeneratorCritic::new("pair", ports, rig.settings.clone(), spec)
}

#[derive(Clone, Copy)]
pub enum CriticStore {
    Fail,
    Hang,
}

pub struct FaultyAccess {
    inner: Arc<dyn ThreadAccess>,
    critic: br_llm_messages::AgentId,
    mode: CriticStore,
    pub reached: Arc<AtomicBool>,
}

impl FaultyAccess {
    pub fn new(
        inner: Arc<dyn ThreadAccess>,
        critic: br_llm_messages::AgentId,
        mode: CriticStore,
    ) -> Self {
        Self {
            inner,
            critic,
            mode,
            reached: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ThreadAccess for FaultyAccess {
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
        if turn.agent != self.critic {
            return self.inner.complete_response(thread, turn, response);
        }
        let mode = self.mode;
        let reached = self.reached.clone();
        Box::pin(async move {
            reached.store(true, Ordering::SeqCst);
            match mode {
                CriticStore::Fail => Err(PortError::fault("the thread access is down")),
                CriticStore::Hang => std::future::pending().await,
            }
        })
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
