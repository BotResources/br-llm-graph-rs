use std::future::Future;
use std::pin::Pin;

use br_llm_messages::ToolCallId;
use serde_json::Value;

use crate::state::{Config, State};

/// The error of a hook, of the host's type: it fails the tool node and
/// reaches the caller as `NodeFault::Returned`, where the host can downcast
/// it.
pub type HookError = Box<dyn std::error::Error + Send + Sync>;

/// The judgment of one call under way: a `Judgment`, or the hook's error.
pub type HookFuture<'a> = Pin<Box<dyn Future<Output = Result<Judgment, HookError>> + Send + 'a>>;

/// What a hook decides for one tool call. More judgments may come in a
/// later version: a `match` on it outside the crate needs a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Judgment {
    /// The call runs.
    Accept,
    /// The call is not executed: it is answered with an error result holding
    /// `reason` (`tool call refused` when the reason is blank), and the turn
    /// goes on.
    Refuse {
        /// Why the call is refused.
        reason: String,
    },
}

/// The judgment of one tool call, made before its tool runs. A hook is the
/// host's: the graph only runs it.
///
/// A call is judged after the `enabled` key check (a call naming a tool that
/// is not enabled is answered without its hook), inside its slot of the tool
/// concurrency limit, before its tool runs. A hook returns no update. Its
/// error fails the tool node, as returned, and the call is not executed: no
/// call of the node that has not started is started afterwards, the calls
/// already started (judging or running) finish, and the node's results are
/// dropped; the error returned is the first, in call order, of the hooks
/// that failed.
///
/// What a host's hook must allow for:
/// - An error is not a strong refusal. Use `Judgment::Refuse` for a policy
///   decision; return an error only when the hook cannot judge. The error
///   fails the node and the run, and a resumed run judges the call again.
/// - A judgment under way is dropped when the run is cancelled: a hook that
///   does I/O must be cancel-safe.
/// - A pending judgment holds its call's concurrency slot, and the tool
///   nodes of one ReAct loop share one limit per superstep: a slow hook slows
///   down every call of the superstep waiting for a slot, in any of those
///   tool nodes.
/// - A call may be judged more than once, under the same `ToolCallId`, after
///   a cancel or a failed node (a failed node records nothing, so a resumed
///   run judges and runs its calls again): an approval hook memoises its
///   judgment by call id.
/// - The effects of tools are at-least-once across a resume: a call whose
///   tool ran before its node failed, or before a cancel, runs again.
/// - A hook that panics fails the node as a panic (`NodeFault::Panic`); the
///   other calls of the node are dropped at once.
pub trait Hook: Send + Sync {
    /// Judges the call `id` with its JSON `arguments`, the state and the
    /// configuration: it accepts or refuses it, and returns no update. Its
    /// error fails the node.
    fn judge<'a>(
        &'a self,
        id: &'a ToolCallId,
        arguments: &'a Value,
        state: &'a State,
        config: &'a Config,
    ) -> HookFuture<'a>;
}
