use br_agentic_contract::{PortError, PortFuture};

use super::MemorySandbox;
use super::claim::Claim;
use super::state::{BoxState, SaveKey, SaveState, State};
use crate::memory::record::HarnessCall;
use crate::open::{OpenRequest, Opened};
use crate::path::FilePath;
use crate::sandbox::{BoxId, Command, CommandResult, ProcessId, RunMode, RunOutcome, SandboxPort};
use crate::save::{AlreadySaved, SaveReceipt};

impl MemorySandbox {
    fn with_box<T>(
        &self,
        sandbox: &BoxId,
        act: impl FnOnce(&mut BoxState, &mut State) -> Result<T, PortError>,
    ) -> Result<T, PortError> {
        let mut state = self.lock();
        let mut held = state
            .boxes
            .remove(sandbox)
            .ok_or_else(|| PortError::fault(format!("the box {sandbox} was never opened")))?;
        let result = if held.released {
            Err(PortError::fault(format!("the box {sandbox} is released")))
        } else {
            act(&mut held, &mut state)
        };
        state.boxes.insert(sandbox.clone(), held);
        result
    }
}

impl SandboxPort for MemorySandbox {
    fn open<'a>(&'a self, request: &'a OpenRequest) -> PortFuture<'a, Result<Opened, PortError>> {
        self.record.push(HarnessCall::SandboxOpen {
            request: request.clone(),
        });
        Box::pin(async move {
            let begun = self.lock().begin(request)?;
            let claim = Claim::new(self, begun.sandbox.clone(), begun.kept);
            if let Some(hold) = &begun.hold {
                hold.reach(begun.sandbox.clone());
                hold.proceeded().await;
            }
            claim.keep();
            Ok(Opened::new(begun.sandbox, begun.start))
        })
    }

    fn run<'a>(
        &'a self,
        sandbox: &'a BoxId,
        command: &'a Command,
        mode: RunMode,
    ) -> PortFuture<'a, Result<RunOutcome, PortError>> {
        self.record.push(HarnessCall::SandboxRun {
            sandbox: sandbox.clone(),
            command: command.clone(),
            mode,
        });
        Box::pin(async move {
            self.with_box(sandbox, |held, state| match mode {
                RunMode::Background => {
                    let process = ProcessId::new(state.mint("process"));
                    held.running.insert(process.clone());
                    Ok(RunOutcome::Started(process))
                }
                RunMode::Foreground => Ok(RunOutcome::Finished(
                    state
                        .scripts
                        .get(command.as_str())
                        .cloned()
                        .unwrap_or_else(|| CommandResult {
                            exit_code: 127,
                            stdout: Vec::new(),
                            stderr: b"command not scripted".to_vec(),
                        }),
                )),
            })
        })
    }

    fn read<'a>(
        &'a self,
        sandbox: &'a BoxId,
        path: &'a FilePath,
    ) -> PortFuture<'a, Result<Vec<u8>, PortError>> {
        self.record.push(HarnessCall::SandboxRead {
            sandbox: sandbox.clone(),
            path: path.clone(),
        });
        Box::pin(async move {
            self.with_box(sandbox, |held, _| {
                held.files
                    .get(path)
                    .cloned()
                    .ok_or_else(|| PortError::refused(format!("the file {path} does not exist")))
            })
        })
    }

    fn write<'a>(
        &'a self,
        sandbox: &'a BoxId,
        path: &'a FilePath,
        content: Vec<u8>,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(HarnessCall::SandboxWrite {
            sandbox: sandbox.clone(),
            path: path.clone(),
            content: content.clone(),
        });
        Box::pin(async move {
            self.with_box(sandbox, |held, _| {
                held.files.insert(path.clone(), content);
                Ok(())
            })
        })
    }

    fn list<'a>(
        &'a self,
        sandbox: &'a BoxId,
        under: Option<&'a FilePath>,
    ) -> PortFuture<'a, Result<Vec<FilePath>, PortError>> {
        self.record.push(HarnessCall::SandboxList {
            sandbox: sandbox.clone(),
            under: under.cloned(),
        });
        Box::pin(async move {
            self.with_box(sandbox, |held, _| {
                Ok(held
                    .files
                    .keys()
                    .filter(|path| under.is_none_or(|folder| path.is_under(folder)))
                    .cloned()
                    .collect())
            })
        })
    }

    fn kill<'a>(
        &'a self,
        sandbox: &'a BoxId,
        process: &'a ProcessId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(HarnessCall::SandboxKill {
            sandbox: sandbox.clone(),
            process: process.clone(),
        });
        Box::pin(async move {
            self.with_box(sandbox, |held, _| {
                held.running.remove(process);
                Ok(())
            })
        })
    }

    fn save<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<SaveReceipt, PortError>> {
        self.record.push(HarnessCall::SandboxSave {
            sandbox: sandbox.clone(),
        });
        Box::pin(async move {
            self.with_box(sandbox, |held, state| {
                if state.store_unavailable {
                    return Err(PortError::fault("the place of the saves is unavailable"));
                }
                let key = SaveKey::of(&held.turn);
                match state.saves.get(&key) {
                    Some(SaveState::Failed) => {
                        return Err(PortError::fault("the save did not finish"));
                    }
                    Some(_) => return Err(AlreadySaved::refusal()),
                    None => {}
                }
                let made_at = state.now();
                let files = held.files.clone();
                let bytes = files.values().map(|content| content.len() as u64).sum();
                state.saves.insert(key, SaveState::Saved { files });
                held.made_save = true;
                Ok(SaveReceipt::new(held.turn.turn, made_at, bytes))
            })
        })
    }

    fn release<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(HarnessCall::SandboxRelease {
            sandbox: sandbox.clone(),
        });
        Box::pin(async move {
            if self.lock().release_box(sandbox) {
                Ok(())
            } else {
                Err(PortError::fault(format!(
                    "the box {sandbox} was never opened"
                )))
            }
        })
    }
}
