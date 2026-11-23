#![doc = include_str!("../README.md")]

mod entry_item;
mod error;
mod future;
mod ids;
mod incoming;
mod mail;
pub mod memory;
mod node;
mod node_ports;
mod outcome;
mod ports;
mod rules;
mod run_context;
mod stream;
mod structure;
mod thread_access;
mod tool;
mod web;

pub use br_graph_contract::{
    CancelView, Cause, Event, Fields, InputReader, InstanceId, Node, Reason,
};
pub use br_llm_clients_core::StreamSink;
pub use entry_item::{EntryItem, InvalidEntryItem};
pub use error::PortError;
pub use future::PortFuture;
pub use ids::{AgentLabel, InvalidLabel, InvalidThreadRef, MailId, ThreadRef};
pub use incoming::{Cursor, IncomingBatch, IncomingContext, IncomingItem, IncomingPort};
pub use mail::{Attachment, DocumentRef, MailDocument, MailItem, NewMail};
pub use node::{AGENTIC, AgenticNode};
pub use node_ports::{
    AuthorPort, CriticRequest, CriticRequestPort, GeneratorCriticOutcomePort, OutputSchemaPort,
    ProgrammaticCriticPort, ProgrammaticVerdict, PromptPort, ReactOutcomePort,
};
pub use outcome::{GeneratorCriticOutcome, Judgement, ReactOutcome};
pub use ports::{AgentPort, ThreadPort, ToolsPort};
pub use rules::{
    ApplyError, apply_closure, apply_entry, apply_input_turn, apply_response, apply_results,
};
pub use run_context::RunContext;
pub use stream::{ResponseStream, ResultStream, ResultStreamBox};
pub use structure::structured_reply;
pub use thread_access::{AiTurnRef, ThreadAccess};
pub use tool::{DuplicateTool, Tool, ToolBlock, ToolCode, ToolContent, ToolFault, ToolSet};
pub use web::{InvalidSite, Site, WebAccess};
