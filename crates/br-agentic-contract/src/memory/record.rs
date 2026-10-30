use std::sync::{Arc, Mutex, PoisonError};

use br_graph_contract::InstanceId;
use br_llm_messages::{
    InputTurn, ModelResponse, ResponseDelta, RuntimeToolResults, TurnClosure, TurnEntry,
};

use crate::ids::{AgentLabel, MailId, ThreadRef};
use crate::inbox::{Cursor, InboxContext};
use crate::mail::NewMail;
use crate::thread_access::AiTurnRef;
use crate::workspace::WorkspaceFile;
use br_llm_messages::EntryId;
use std::num::NonZeroUsize;

#[derive(Debug, Clone)]
pub enum Call {
    ThreadPort,
    AgentPort {
        instance: InstanceId,
        label: AgentLabel,
    },
    ToolsPort {
        instance: InstanceId,
    },
    WorkspaceLoad,
    WorkspaceSave {
        files: Vec<WorkspaceFile>,
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
    InboxItems {
        context: InboxContext,
        after: Option<Cursor>,
        limit: NonZeroUsize,
    },
    InboxWait {
        context: InboxContext,
    },
    InboxMarkRead {
        context: InboxContext,
        item: EntryId,
    },
    InboxSend {
        context: InboxContext,
        mail: NewMail,
    },
    InboxDocuments {
        context: InboxContext,
        mail: MailId,
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
