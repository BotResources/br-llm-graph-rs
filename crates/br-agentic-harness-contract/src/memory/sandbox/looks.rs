use super::state::{SaveKey, SaveState};
use super::{MemorySandbox, Network};
use crate::memory::record::HarnessCall;
use crate::path::FilePath;
use crate::sandbox::{BoxId, ProcessId};
use crate::seed::Seed;
use crate::turn::TurnRef;

type Content = Vec<(FilePath, Vec<u8>)>;

impl MemorySandbox {
    pub fn network(&self, sandbox: &BoxId) -> Option<Network> {
        let state = self.lock();
        let web = state.boxes.get(sandbox)?.web.as_ref();
        Some(match web {
            Some(web) if !web.reaches_nothing() => Network::Open(web.clone()),
            _ => Network::Closed,
        })
    }

    pub fn running(&self, sandbox: &BoxId) -> Vec<ProcessId> {
        self.lock()
            .boxes
            .get(sandbox)
            .map(|held| held.running.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn saved(&self, sandbox: &BoxId) -> Option<Content> {
        let state = self.lock();
        let held = state.boxes.get(sandbox)?;
        saved_content(&state.saves, &SaveKey::of(&held.turn))
    }

    pub fn save_of(&self, turn: &TurnRef) -> Option<Content> {
        saved_content(&self.lock().saves, &SaveKey::of(turn))
    }

    pub fn files(&self, sandbox: &BoxId) -> Option<Content> {
        let state = self.lock();
        let held = state.boxes.get(sandbox)?;
        Some(
            held.files
                .iter()
                .map(|(path, content)| (path.clone(), content.clone()))
                .collect(),
        )
    }

    pub fn seed(&self, sandbox: &BoxId) -> Option<Seed> {
        self.lock().boxes.get(sandbox).map(|held| held.seed.clone())
    }

    pub fn writes(&self, sandbox: &BoxId) -> Content {
        self.record
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                HarnessCall::SandboxWrite {
                    sandbox: written,
                    path,
                    content,
                } if &written == sandbox => Some((path, content)),
                _ => None,
            })
            .collect()
    }

    pub fn is_released(&self, sandbox: &BoxId) -> bool {
        self.lock()
            .boxes
            .get(sandbox)
            .is_some_and(|held| held.released)
    }
}

fn saved_content(
    saves: &std::collections::HashMap<SaveKey, SaveState>,
    key: &SaveKey,
) -> Option<Content> {
    match saves.get(key)? {
        SaveState::Saved { files } => Some(
            files
                .iter()
                .map(|(path, content)| (path.clone(), content.clone()))
                .collect(),
        ),
        SaveState::Expired(_) | SaveState::Failed => None,
    }
}
