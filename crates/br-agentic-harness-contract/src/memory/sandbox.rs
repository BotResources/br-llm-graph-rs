use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use br_agentic_contract::{InstanceId, PortError, PortFuture, WebAccess};

use super::record::{HarnessCall, HarnessRecord};
use crate::path::FilePath;
use crate::sandbox::{BoxId, Command, CommandResult, ProcessId, RunMode, RunOutcome, SandboxPort};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Network {
    Closed,
    Open(WebAccess),
}

struct BoxState {
    web: Option<WebAccess>,
    files: BTreeMap<FilePath, Vec<u8>>,
    running: BTreeSet<ProcessId>,
    saved: Option<BTreeMap<FilePath, Vec<u8>>>,
    released: bool,
}

#[derive(Default)]
struct State {
    boxes: HashMap<BoxId, BoxState>,
    scripts: HashMap<String, CommandResult>,
    next: u64,
}

#[derive(Clone)]
pub struct MemorySandbox {
    record: HarnessRecord,
    state: Arc<Mutex<State>>,
}

impl MemorySandbox {
    pub fn new(record: &HarnessRecord) -> Self {
        Self {
            record: record.clone(),
            state: Arc::default(),
        }
    }

    pub fn script(&self, command: &str, result: CommandResult) {
        self.lock().scripts.insert(command.to_owned(), result);
    }

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

    pub fn end_process(&self, sandbox: &BoxId, process: &ProcessId) {
        if let Some(held) = self.lock().boxes.get_mut(sandbox) {
            held.running.remove(process);
        }
    }

    pub fn saved(&self, sandbox: &BoxId) -> Option<Vec<(FilePath, Vec<u8>)>> {
        let state = self.lock();
        let saved = state.boxes.get(sandbox)?.saved.as_ref()?;
        Some(
            saved
                .iter()
                .map(|(path, content)| (path.clone(), content.clone()))
                .collect(),
        )
    }

    pub fn is_released(&self, sandbox: &BoxId) -> bool {
        self.lock()
            .boxes
            .get(sandbox)
            .is_some_and(|held| held.released)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

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
    fn open<'a>(
        &'a self,
        instance: InstanceId,
        web: Option<&'a WebAccess>,
    ) -> PortFuture<'a, Result<BoxId, PortError>> {
        self.record.push(HarnessCall::SandboxOpen {
            instance,
            web: web.cloned(),
        });
        Box::pin(async move {
            let mut state = self.lock();
            state.next += 1;
            let sandbox = BoxId::new(format!("box-{}", state.next));
            state.boxes.insert(
                sandbox.clone(),
                BoxState {
                    web: web.cloned(),
                    files: BTreeMap::new(),
                    running: BTreeSet::new(),
                    saved: None,
                    released: false,
                },
            );
            Ok(sandbox)
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
                    state.next += 1;
                    let process = ProcessId::new(format!("process-{}", state.next));
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

    fn save<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(HarnessCall::SandboxSave {
            sandbox: sandbox.clone(),
        });
        Box::pin(async move {
            self.with_box(sandbox, |held, _| {
                held.saved = Some(held.files.clone());
                Ok(())
            })
        })
    }

    fn release<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<(), PortError>> {
        self.record.push(HarnessCall::SandboxRelease {
            sandbox: sandbox.clone(),
        });
        Box::pin(async move {
            let mut state = self.lock();
            match state.boxes.get_mut(sandbox) {
                Some(held) => {
                    held.released = true;
                    Ok(())
                }
                None => Err(PortError::fault(format!(
                    "the box {sandbox} was never opened"
                ))),
            }
        })
    }
}
