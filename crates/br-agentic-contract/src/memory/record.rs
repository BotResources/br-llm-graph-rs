use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};

use br_graph_contract::InstanceId;
use br_llm_messages::{
    EntryId, InputTurn, ModelResponse, ResponseDelta, RuntimeToolResults, TurnClosure, TurnEntry,
};

use crate::ids::{AgentLabel, ThreadRef};
use crate::incoming::{Cursor, IncomingContext};
use crate::run_context::RunContext;
use crate::thread_access::AiTurnRef;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Call {
    ThreadPort,
    AgentPort {
        instance: InstanceId,
        label: AgentLabel,
    },
    ToolsPort {
        context: RunContext,
    },
    Read {
        thread: ThreadRef,
    },
    AddInputTurn {
        thread: ThreadRef,
        turn: InputTurn,
    },
    AddEntry {
        thread: ThreadRef,
        turn: AiTurnRef,
        entry: TurnEntry,
    },
    CloseTurn {
        thread: ThreadRef,
        turn: AiTurnRef,
        closure: TurnClosure,
    },
    OpenResponse {
        thread: ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    },
    ResponseDelta {
        entry: EntryId,
        delta: ResponseDelta,
    },
    CompleteResponse {
        thread: ThreadRef,
        turn: AiTurnRef,
        response: ModelResponse,
    },
    DiscardResponse {
        thread: ThreadRef,
        entry: EntryId,
    },
    OpenResult {
        thread: ThreadRef,
        turn: AiTurnRef,
        entry: EntryId,
    },
    ResultFragment {
        entry: EntryId,
        fragment: String,
    },
    CompleteResult {
        thread: ThreadRef,
        turn: AiTurnRef,
        result: RuntimeToolResults,
    },
    DiscardResult {
        thread: ThreadRef,
        entry: EntryId,
    },
    IncomingItems {
        context: IncomingContext,
        after: Option<Cursor>,
        limit: NonZeroUsize,
    },
    IncomingMarkRead {
        context: IncomingContext,
        item: EntryId,
    },
}

#[derive(Debug, Clone, Default)]
pub struct Record {
    calls: Arc<Mutex<Vec<Call>>>,
}

impl Record {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn push(&self, call: Call) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
}
