use std::future::Future;
use std::pin::Pin;

use br_llm_messages::ToolCallId;
use serde_json::Value;

use crate::state::{Config, State};

pub type HookError = Box<dyn std::error::Error + Send + Sync>;

pub type HookFuture<'a> = Pin<Box<dyn Future<Output = Result<Judgment, HookError>> + Send + 'a>>;

/// What a hook decides for one tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judgment {
    /// The call runs.
    Accept,
    /// The call is not executed: it is answered with an error result holding
    /// `reason` (`tool call refused` when the reason is blank), and the turn
    /// goes on.
    Refuse { reason: String },
}

/// The judgment of one tool call, made before its tool runs. A hook is the
/// host's: the graph only runs it.
///
/// A call is judged after the `enabled` key check (a call naming a tool that
/// is not enabled is answered without its hook), inside its slot of the tool
/// concurrency limit, before its tool runs. A hook returns no update. Its
/// error fails the tool node, as returned, and the call is not executed.
pub trait Hook: Send + Sync {
    fn judge<'a>(
        &'a self,
        id: &'a ToolCallId,
        arguments: &'a Value,
        state: &'a State,
        config: &'a Config,
    ) -> HookFuture<'a>;
}
