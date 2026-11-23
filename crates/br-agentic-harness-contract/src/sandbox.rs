use std::fmt;

use br_agentic_contract::{InstanceId, PortError, PortFuture, WebAccess};

use crate::path::FilePath;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BoxId(String);

impl BoxId {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BoxId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessId(String);

impl ProcessId {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProcessId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Command(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a command is a non-blank text")]
pub struct InvalidCommand;

impl Command {
    pub fn new(text: impl Into<String>) -> Result<Self, InvalidCommand> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(InvalidCommand);
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Foreground,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandResult {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Finished(CommandResult),
    Started(ProcessId),
}

pub trait SandboxPort: Send + Sync {
    fn open<'a>(
        &'a self,
        instance: InstanceId,
        web: Option<&'a WebAccess>,
    ) -> PortFuture<'a, Result<BoxId, PortError>>;

    fn run<'a>(
        &'a self,
        sandbox: &'a BoxId,
        command: &'a Command,
        mode: RunMode,
    ) -> PortFuture<'a, Result<RunOutcome, PortError>>;

    fn read<'a>(
        &'a self,
        sandbox: &'a BoxId,
        path: &'a FilePath,
    ) -> PortFuture<'a, Result<Vec<u8>, PortError>>;

    fn write<'a>(
        &'a self,
        sandbox: &'a BoxId,
        path: &'a FilePath,
        content: Vec<u8>,
    ) -> PortFuture<'a, Result<(), PortError>>;

    fn list<'a>(
        &'a self,
        sandbox: &'a BoxId,
        under: Option<&'a FilePath>,
    ) -> PortFuture<'a, Result<Vec<FilePath>, PortError>>;

    fn kill<'a>(
        &'a self,
        sandbox: &'a BoxId,
        process: &'a ProcessId,
    ) -> PortFuture<'a, Result<(), PortError>>;

    fn save<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<(), PortError>>;

    fn release<'a>(&'a self, sandbox: &'a BoxId) -> PortFuture<'a, Result<(), PortError>>;
}
