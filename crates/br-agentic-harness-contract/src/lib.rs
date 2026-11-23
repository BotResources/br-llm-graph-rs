#![doc = include_str!("../README.md")]

mod history;
mod lifecycle;
mod mail;
pub mod memory;
mod path;
mod sandbox;
mod skills;

pub use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, IncomingItem, InstanceId, MailDocument, MailId,
    NewMail, PortError, PortFuture, PromptPort, RunContext, Site, ThreadAccess, ThreadRef,
    ToolsPort, WebAccess,
};
pub use history::{HarnessHistory, StoredAnswer};
pub use lifecycle::{RunEnd, RunLifecycle};
pub use mail::MailPort;
pub use path::{FilePath, InvalidFilePath};
pub use sandbox::{
    BoxId, Command, CommandResult, InvalidCommand, ProcessId, RunMode, RunOutcome, SandboxPort,
};
pub use skills::{InvalidSkillName, SkillName, SkillSummary, SkillsPort};
