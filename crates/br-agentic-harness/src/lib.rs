#![doc = include_str!("../README.md")]

mod get_mail_files;
mod harness;
mod history;
mod incoming;
mod lifecycle;
mod mail_set;
mod mail_spec;
mod mail_tools;
mod prompt;
mod runs;
mod send_mail;
mod shared;
mod tool_text;
mod tools;

pub use br_agentic_harness_contract::{
    BoxId, HarnessHistory, MailPort, ProcessId, RunEnd, RunLifecycle, SandboxPort, SkillsPort,
};
pub use harness::{Harness, HarnessBuilder};
pub use history::HistoryAdapter;
pub use incoming::IncomingAdapter;
pub use mail_spec::{GET_MAIL_FILES, SEND_MAIL};
pub use prompt::PromptAdapter;
pub use tools::ToolsAdapter;
