use std::sync::{Arc, Mutex, PoisonError};

use br_agentic_contract::{PortError, PortFuture};
use br_agentic_harness::{BoxId, ProcessId, SandboxPort};
use br_agentic_harness_contract::memory::MemorySandbox;
use br_agentic_harness_contract::{
    Command, FilePath, OpenRequest, Opened, RunMode, RunOutcome, SaveReceipt,
};

#[derive(Clone, Default)]
pub struct Seen(Arc<Mutex<Vec<String>>>);

impl Seen {
    pub fn all(&self) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub fn push(&self, line: String) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
}

pub struct Spy {
    inner: MemorySandbox,
    seen: Seen,
}

pub fn spy(inner: &MemorySandbox, seen: &Seen) -> Arc<dyn SandboxPort> {
    Arc::new(Spy {
        inner: inner.clone(),
        seen: seen.clone(),
    })
}

impl SandboxPort for Spy {
    fn open<'a>(&'a self, request: &'a OpenRequest) -> PortFuture<'a, Result<Opened, PortError>> {
        self.seen.push(format!("open {request:?}"));
        self.inner.open(request)
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
        self.seen.push(format!("read {sandbox:?} {path:?}"));
        self.inner.read(sandbox, path)
    }

    fn write<'a>(
        &'a self,
        sandbox: &'a BoxId,
        path: &'a FilePath,
        content: Vec<u8>,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.seen.push(format!(
            "write {sandbox:?} {path:?} {}",
            String::from_utf8_lossy(&content)
        ));
        self.inner.write(sandbox, path, content)
    }

    fn list<'a>(
        &'a self,
        sandbox: &'a BoxId,
        under: Option<&'a FilePath>,
    ) -> PortFuture<'a, Result<Vec<FilePath>, PortError>> {
        self.seen.push(format!("list {sandbox:?} {under:?}"));
        self.inner.list(sandbox, under)
    }

    fn kill<'a>(
        &'a self,
        sandbox: &'a BoxId,
        process: &'a ProcessId,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.seen.push(format!("kill {sandbox:?} {process:?}"));
        self.inner.kill(sandbox, process)
    }

    fn save<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<SaveReceipt, PortError>> {
        self.seen.push(format!("save {sandbox:?}"));
        self.inner.save(sandbox)
    }

    fn release<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<(), PortError>> {
        self.seen.push(format!("release {sandbox:?}"));
        self.inner.release(sandbox)
    }
}
