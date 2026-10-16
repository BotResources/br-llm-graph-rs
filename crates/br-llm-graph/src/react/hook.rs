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
///   fails the node and the run (see below for what a resumed run does).
/// - A judgment under way is dropped when the run is cancelled: a hook that
///   does I/O must be cancel-safe.
/// - A pending judgment holds its call's concurrency slot, and the tool
///   nodes of one ReAct loop share one limit per superstep: a slow hook slows
///   down every call of the superstep waiting for a slot, in any of those
///   tool nodes.
/// - A failed or cancelled tool node records nothing: the graph that holds
///   it, resumed from its checkpoint, runs the node again, which judges and
///   runs its calls again under the same `ToolCallId`s. The tool nodes are
///   an agent's, and an agent's graph is resumed through its entry, so for
///   an agent:
///   - durable, a call is judged at most once: a call left without a result
///     is answered as interrupted when the agent enters its thread again,
///     and is neither judged nor run again; the model reads that result and
///     decides whether to call again;
///   - in memory, the call restarts from `opening`: the model is asked
///     again, and the calls of its new reply are judged, possibly under the
///     ids of the calls before. An approval hook memoises its judgment by
///     call id.
/// - The effects of tools are at-least-once across a resume in memory: a
///   call whose tool ran before its node failed, or before a cancel, runs
///   again when the model calls it again. Durable, its result was not
///   written, so it is answered as interrupted and not run again by itself.
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
