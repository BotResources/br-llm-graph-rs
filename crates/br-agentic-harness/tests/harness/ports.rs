use std::sync::{Arc, Mutex, PoisonError};

use br_agentic_contract::{PortError, PortFuture};
use br_agentic_harness::{BoxId, ProcessId, SandboxPort};
use br_agentic_harness_contract::memory::MemorySandbox;
use br_agentic_harness_contract::{
    AlreadySaved, Command, FilePath, OpenFault, OpenRequest, Opened, RunMode, RunOutcome,
    SaveReceipt, StartKind,
};
use tokio::sync::Notify;

use crate::spy::Seen;

pub const KILL_FAILURE: &str = "the process cannot be killed";
pub const SAVE_FAILURE: &str = "the folder cannot be saved";
const OTHER_REFUSAL: &str = "the save is refused for another reason";

#[derive(Clone, Copy)]
pub enum SaveAnswer {
    Works,
    Fault,
    AlreadySaved,
    OtherRefusal,
    NearMissRefusal,
}

#[derive(Clone, Copy)]
pub enum OpenAnswer {
    Real,
    Start(StartKind),
    Image(OpenFault),
    PlainFault,
    Refusal,
}

#[derive(Clone, Default)]
pub struct Gate {
    entered: Arc<Notify>,
    go: Arc<Notify>,
}

impl Gate {
    pub async fn entered(&self) {
        self.entered.notified().await;
    }

    pub fn go(&self) {
        self.go.notify_one();
    }
}

#[derive(Clone)]
pub struct Rigged {
    inner: MemorySandbox,
    seen: Seen,
    kill_fails: bool,
    save: SaveAnswer,
    open: OpenAnswer,
    gate: Option<Gate>,
    slow_open: bool,
    boxes: Arc<Mutex<Vec<BoxId>>>,
}

impl Rigged {
    pub fn new(inner: &MemorySandbox, seen: &Seen) -> Self {
        Self {
            inner: inner.clone(),
            seen: seen.clone(),
            kill_fails: false,
            save: SaveAnswer::Works,
            open: OpenAnswer::Real,
            gate: None,
            slow_open: false,
            boxes: Arc::default(),
        }
    }

    pub fn boxes(&self) -> Vec<BoxId> {
        self.boxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn kill_fails(mut self) -> Self {
        self.kill_fails = true;
        self
    }

    pub fn save(mut self, answer: SaveAnswer) -> Self {
        self.save = answer;
        self
    }

    pub fn open(mut self, answer: OpenAnswer) -> Self {
        self.open = answer;
        self
    }

    pub fn gated(mut self) -> (Self, Gate) {
        let gate = Gate::default();
        self.gate = Some(gate.clone());
        (self, gate)
    }

    pub fn slow_open(mut self) -> Self {
        self.slow_open = true;
        self
    }

    pub fn port(self) -> Arc<dyn SandboxPort> {
        Arc::new(self)
    }
}

impl Rigged {
    fn made(&self, opened: &Opened) {
        self.boxes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(opened.box_id.clone());
    }
}

impl SandboxPort for Rigged {
    fn open<'a>(&'a self, request: &'a OpenRequest) -> PortFuture<'a, Result<Opened, PortError>> {
        self.seen.push(format!("open {request:?}"));
        Box::pin(async move {
            if let Some(gate) = &self.gate {
                gate.entered.notify_one();
                gate.go.notified().await;
            }
            if self.slow_open {
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
            }
            match self.open {
                OpenAnswer::Real => {
                    let opened = self.inner.open(request).await?;
                    self.made(&opened);
                    Ok(opened)
                }
                OpenAnswer::Start(start) => {
                    let opened = self.inner.open(request).await?;
                    self.made(&opened);
                    Ok(Opened::new(opened.box_id, start))
                }
                OpenAnswer::Image(fault) => Err(fault.into()),
                OpenAnswer::PlainFault => Err(PortError::fault("the provider is down")),
                OpenAnswer::Refusal => Err(PortError::refused("the provider refuses")),
            }
        })
    }

    fn run<'a>(
        &'a self,
        sandbox: &'a BoxId,
        command: &'a Command,
        mode: RunMode,
    ) -> PortFuture<'a, Result<RunOutcome, PortError>> {
        self.seen
            .push(format!("run {sandbox:?} {command:?} {mode:?}"));
        self.inner.run(sandbox, command, mode)
    }

    fn read<'a>(
        &'a self,
        sandbox: &'a BoxId,
        path: &'a FilePath,
    ) -> PortFuture<'a, Result<Vec<u8>, PortError>> {
        self.inner.read(sandbox, path)
    }

    fn write<'a>(
        &'a self,
        sandbox: &'a BoxId,
        path: &'a FilePath,
        content: Vec<u8>,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.inner.write(sandbox, path, content)
    }

    fn list<'a>(
        &'a self,
        sandbox: &'a BoxId,
        under: Option<&'a FilePath>,
    ) -> PortFuture<'a, Result<Vec<FilePath>, PortError>> {
        self.inner.list(sandbox, under)
    }

    fn kill<'a>(
        &'a self,
        sandbox: &'a BoxId,
        process: &'a ProcessId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.seen.push(format!("kill {sandbox:?} {process:?}"));
        if self.kill_fails {
            return Box::pin(async move { Err(PortError::fault(KILL_FAILURE)) });
        }
        self.inner.kill(sandbox, process)
    }

    fn save<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<SaveReceipt, PortError>> {
        self.seen.push(format!("save {sandbox:?}"));
        match self.save {
            SaveAnswer::Works => self.inner.save(sandbox),
            SaveAnswer::Fault => Box::pin(async move { Err(PortError::fault(SAVE_FAILURE)) }),
            SaveAnswer::AlreadySaved => Box::pin(async move { Err(AlreadySaved::refusal()) }),
            SaveAnswer::OtherRefusal => {
                Box::pin(async move { Err(PortError::refused(OTHER_REFUSAL)) })
            }
            SaveAnswer::NearMissRefusal => {
                Box::pin(
                    async move { Err(PortError::refused(format!("{}.", AlreadySaved::REASON))) },
                )
            }
        }
    }

    fn release<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<(), PortError>> {
        self.seen.push(format!("release {sandbox:?}"));
        self.inner.release(sandbox)
    }
}
