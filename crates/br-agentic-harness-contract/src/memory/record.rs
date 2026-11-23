use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, PoisonError};

use br_agentic_contract::{Cursor, IncomingContext, InstanceId, MailId, ThreadRef, WebAccess};
use br_llm_messages::EntryId;

use crate::path::FilePath;
use crate::sandbox::{BoxId, Command, ProcessId, RunMode};
use crate::skills::SkillName;

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum HarnessCall {
    Stored {
        thread: ThreadRef,
        entry: EntryId,
    },
    MailItems {
        context: IncomingContext,
        after: Option<Cursor>,
        limit: NonZeroUsize,
    },
    MailMarkRead {
        context: IncomingContext,
        item: EntryId,
    },
    MailSend {
        context: IncomingContext,
        mail: MailId,
    },
    MailDocuments {
        context: IncomingContext,
        mail: MailId,
    },
    SkillsList {
        instance: InstanceId,
    },
    SkillsRead {
        instance: InstanceId,
        skill: SkillName,
        path: FilePath,
    },
    SandboxOpen {
        instance: InstanceId,
        web: Option<WebAccess>,
    },
    SandboxRun {
        sandbox: BoxId,
        command: Command,
        mode: RunMode,
    },
    SandboxRead {
        sandbox: BoxId,
        path: FilePath,
    },
    SandboxWrite {
        sandbox: BoxId,
        path: FilePath,
    },
    SandboxList {
        sandbox: BoxId,
        under: Option<FilePath>,
    },
    SandboxKill {
        sandbox: BoxId,
        process: ProcessId,
    },
    SandboxSave {
        sandbox: BoxId,
    },
    SandboxRelease {
        sandbox: BoxId,
    },
}

#[derive(Debug, Clone, Default)]
pub struct HarnessRecord {
    calls: Arc<Mutex<Vec<HarnessCall>>>,
}

impl HarnessRecord {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<HarnessCall> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn push(&self, call: HarnessCall) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }
}
