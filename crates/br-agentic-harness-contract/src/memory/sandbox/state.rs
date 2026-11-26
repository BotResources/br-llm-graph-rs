use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::SystemTime;

use br_agentic_contract::{InstanceId, PortError, ThreadRef, WebAccess};
use br_llm_messages::{AgentId, TurnId};

use super::hold::OpenHold;
use crate::fault::OpenFault;
use crate::open::OpenRequest;
use crate::path::FilePath;
use crate::sandbox::{BoxId, CommandResult, ProcessId};
use crate::seed::Seed;
use crate::start::{FreshReason, StartKind};
use crate::turn::TurnRef;

pub(super) type Files = BTreeMap<FilePath, Vec<u8>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct SaveKey {
    thread: ThreadRef,
    agent: AgentId,
    turn: TurnId,
}

impl SaveKey {
    pub(super) fn of(turn: &TurnRef) -> Self {
        Self {
            thread: turn.thread,
            agent: turn.agent,
            turn: turn.turn,
        }
    }

    fn parent_of(turn: &TurnRef) -> Option<Self> {
        turn.parent.map(|parent| Self {
            thread: turn.thread,
            agent: turn.agent,
            turn: parent,
        })
    }
}

pub(super) enum SaveState {
    Saved { files: Files },
    Expired(Option<SystemTime>),
    Failed,
}

pub(super) struct BoxState {
    pub(super) instance: InstanceId,
    pub(super) turn: TurnRef,
    pub(super) web: Option<WebAccess>,
    pub(super) seed: Seed,
    pub(super) files: Files,
    pub(super) running: BTreeSet<ProcessId>,
    pub(super) made_save: bool,
    pub(super) released: bool,
}

pub(super) struct Begun {
    pub(super) sandbox: BoxId,
    pub(super) start: StartKind,
    pub(super) hold: Option<OpenHold>,
}

#[derive(Default)]
pub(super) struct State {
    pub(super) boxes: HashMap<BoxId, BoxState>,
    pub(super) claims: HashMap<InstanceId, BoxId>,
    pub(super) saves: HashMap<SaveKey, SaveState>,
    pub(super) kept: HashMap<SaveKey, BoxId>,
    pub(super) failed_restores: HashSet<SaveKey>,
    pub(super) store_unavailable: bool,
    pub(super) next_open_fault: Option<OpenFault>,
    pub(super) hold: Option<OpenHold>,
    pub(super) scripts: HashMap<String, CommandResult>,
    pub(super) clock: Option<SystemTime>,
    pub(super) next: u64,
}

impl State {
    pub(super) fn now(&self) -> SystemTime {
        self.clock.unwrap_or_else(SystemTime::now)
    }

    pub(super) fn mint(&mut self, prefix: &str) -> String {
        self.next += 1;
        format!("{prefix}-{}", self.next)
    }

    pub(super) fn begin(&mut self, request: &OpenRequest) -> Result<Begun, PortError> {
        if let Some(fault) = self.next_open_fault.take() {
            return Err(fault.into());
        }
        if self.claims.contains_key(&request.instance) {
            return Err(PortError::refused("the instance already holds an open box"));
        }
        let (start, files, kept) = self.resolve(&request.turn);
        let sandbox = match kept {
            Some(sandbox) => sandbox,
            None => BoxId::new(self.mint("box")),
        };
        self.boxes.insert(
            sandbox.clone(),
            BoxState {
                instance: request.instance,
                turn: request.turn,
                web: request.web_access.clone(),
                seed: request.seed.clone(),
                files,
                running: BTreeSet::new(),
                made_save: false,
                released: false,
            },
        );
        self.claims.insert(request.instance, sandbox.clone());
        Ok(Begun {
            sandbox,
            start,
            hold: self.hold.take(),
        })
    }

    fn resolve(&mut self, turn: &TurnRef) -> (StartKind, Files, Option<BoxId>) {
        let fresh = |reason| (StartKind::Fresh(reason), Files::new(), None);
        let Some(key) = SaveKey::parent_of(turn) else {
            return fresh(FreshReason::NoSaveEver);
        };
        let parent = key.turn;
        if self.store_unavailable {
            return fresh(FreshReason::StoreUnavailable);
        }
        match self.saves.get(&key) {
            None => fresh(FreshReason::NoSaveEver),
            Some(SaveState::Expired(expired_at)) => fresh(FreshReason::SaveExpired {
                expired_at: *expired_at,
            }),
            Some(SaveState::Failed) => fresh(FreshReason::SaveFailed { turn: parent }),
            Some(SaveState::Saved { files }) => {
                let files = files.clone();
                if let Some(sandbox) = self.kept.remove(&key) {
                    (StartKind::Continued, files, Some(sandbox))
                } else if self.failed_restores.contains(&key) {
                    fresh(FreshReason::RestoreFailed { turn: parent })
                } else {
                    (StartKind::Restored { turn: parent }, files, None)
                }
            }
        }
    }

    pub(super) fn release_box(&mut self, sandbox: &BoxId) -> bool {
        let Some(held) = self.boxes.get_mut(sandbox) else {
            return false;
        };
        if held.released {
            return true;
        }
        held.released = true;
        let (instance, key, made_save) = (held.instance, SaveKey::of(&held.turn), held.made_save);
        if self.claims.get(&instance) == Some(sandbox) {
            self.claims.remove(&instance);
        }
        if made_save && matches!(self.saves.get(&key), Some(SaveState::Saved { .. })) {
            self.kept.insert(key, sandbox.clone());
        }
        true
    }
}
