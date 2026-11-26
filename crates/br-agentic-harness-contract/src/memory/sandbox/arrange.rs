use std::time::SystemTime;

use super::MemorySandbox;
use super::hold::OpenHold;
use super::state::{SaveKey, SaveState};
use crate::fault::OpenFault;
use crate::sandbox::{BoxId, CommandResult, ProcessId};
use crate::turn::TurnRef;

impl MemorySandbox {
    pub fn script(&self, command: &str, result: CommandResult) {
        self.lock().scripts.insert(command.to_owned(), result);
    }

    pub fn end_process(&self, sandbox: &BoxId, process: &ProcessId) {
        if let Some(held) = self.lock().boxes.get_mut(sandbox) {
            held.running.remove(process);
        }
    }

    pub fn set_clock(&self, now: SystemTime) {
        self.lock().clock = Some(now);
    }

    pub fn expire_save(&self, turn: &TurnRef, expired_at: Option<SystemTime>) {
        let mut state = self.lock();
        let key = SaveKey::of(turn);
        if matches!(state.saves.get(&key), Some(SaveState::Saved { .. })) {
            state.saves.insert(key, SaveState::Expired(expired_at));
        }
    }

    pub fn forget_save(&self, turn: &TurnRef) {
        let mut state = self.lock();
        let key = SaveKey::of(turn);
        state.saves.remove(&key);
        state.kept.remove(&key);
        state.failed_restores.remove(&key);
    }

    pub fn fail_save(&self, turn: &TurnRef) {
        let mut state = self.lock();
        let key = SaveKey::of(turn);
        state.saves.insert(key, SaveState::Failed);
        state.kept.remove(&key);
    }

    pub fn fail_restore(&self, turn: &TurnRef) {
        self.lock().failed_restores.insert(SaveKey::of(turn));
    }

    pub fn make_store_unavailable(&self) {
        self.lock().store_unavailable = true;
    }

    pub fn end_keep(&self, turn: &TurnRef) {
        self.lock().kept.remove(&SaveKey::of(turn));
    }

    pub fn fail_next_open(&self, fault: OpenFault) {
        self.lock().next_open_fault = Some(fault);
    }

    pub fn hold_next_open(&self) -> OpenHold {
        let hold = OpenHold::new();
        self.lock().hold = Some(hold.clone());
        hold
    }
}
