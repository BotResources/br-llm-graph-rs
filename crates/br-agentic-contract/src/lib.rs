#![doc = include_str!("../README.md")]

mod boundary;
mod documents;
mod ending;
mod error;
mod future;
mod ids;
mod inbox;
mod inbox_entry;
mod mail;
pub mod memory;
mod node;
mod ports;
mod queue;
mod rules;
mod session;
mod stream;
mod thread_access;
mod tool;
mod web;
mod workspace;

pub use boundary::{Boundary, BoundaryError, MailEntryBuilder, store_at_boundary};
pub use br_graph_contract::{
    CancelView, Cause, Event, Fields, InputReader, InstanceId, Node, Reason,
};
pub use documents::mail_documents;
pub use ending::{cancelled, failed, result};
pub use error::PortError;
pub use future::PortFuture;
pub use ids::{AgentLabel, InvalidLabel, InvalidThreadRef, MailId, ThreadRef};
pub use inbox::{Cursor, InboxBatch, InboxContext, InboxItem, InboxPort};
pub use inbox_entry::{InboxEntry, InvalidInboxEntry};
pub use mail::{Attachment, DocumentRef, MailDocument, MailItem, NewMail};
pub use node::{AGENTIC, AgenticNode};
pub use ports::{AgentPort, ThreadPort, ToolsPort};
pub use queue::Queue;
pub use rules::SessionError;
pub use session::Session;
pub use stream::{ResponseStream, ResultStream, ResultStreamBox};
pub use thread_access::{AiTurnRef, ThreadAccess};
pub use tool::{DuplicateTool, Tool, ToolBlock, ToolCode, ToolContent, ToolFault, ToolSet};
pub use web::{InvalidSite, Site, WebAccess};
pub use workspace::{FileSystemMode, WorkspaceFile, WorkspacePort, save_files};
