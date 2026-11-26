#![doc = include_str!("../README.md")]

mod fault;
mod history;
mod lifecycle;
mod mail;
pub mod memory;
mod open;
mod path;
mod sandbox;
mod save;
mod seed;
mod skills;
mod start;
mod turn;

pub use br_agentic_contract::{
    Cursor, IncomingBatch, IncomingContext, IncomingItem, InstanceId, MailDocument, MailId,
    NewMail, PortError, PortFuture, PromptPort, RunContext, Site, ThreadAccess, ThreadRef,
    ToolsPort, WebAccess,
};
pub use br_llm_messages::{AgentId, TurnId};
pub use fault::OpenFault;
pub use history::{HarnessHistory, StoredAnswer};
pub use lifecycle::{RunEnd, RunLifecycle};
pub use mail::MailPort;
pub use open::{OpenRequest, Opened};
pub use path::{FilePath, InvalidFilePath};
pub use sandbox::{
    BoxId, Command, CommandResult, InvalidCommand, ProcessId, RunMode, RunOutcome, SandboxPort,
};
pub use save::{AlreadySaved, SaveReceipt};
pub use seed::{ContentHash, InvalidSeed, Seed, SeedEntry, SeedKind};
pub use skills::{InvalidSkillName, SkillName, SkillSummary, SkillsPort};
pub use start::{FreshReason, StartKind};
pub use turn::TurnRef;
