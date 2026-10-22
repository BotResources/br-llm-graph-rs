//! The runner port, the host's port through which a runner call reaches
//! another runner: the call it takes, named by its key, and what it gives
//! back.

use std::future::Future;
use std::pin::Pin;

use serde_json::{Map, Value};

use crate::runner::answer::RunnerAnswer;
use crate::runner::key::CallKey;
use crate::runner::target::RunnerTarget;

/// The error of a runner port, of the host's type: it fails a runner call
/// node with `GraphError::Runner`, where the host can downcast it; for a
/// runner tool it is a tool error, answered with an error result holding its
/// message.
pub type RunnerError = Box<dyn std::error::Error + Send + Sync>;

/// One call of a runner port under way.
pub type RunnerFuture<'a> =
    Pin<Box<dyn Future<Output = Result<RunnerReply, RunnerError>> + Send + 'a>>;

/// One call a runner port takes: its key, its target and its arguments.
///
/// Its fields are read by the host's port and built with `new`: it may gain
/// fields in a later version.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RunnerRequest {
    /// What names the call across attempts.
    pub key: CallKey,
    /// The runner type of the runner it calls.
    pub target: RunnerTarget,
    /// What the child is given, a JSON object.
    pub arguments: Map<String, Value>,
}

impl RunnerRequest {
    /// The call `key` of the runner `target` with `arguments`.
    pub fn new(key: CallKey, target: RunnerTarget, arguments: Map<String, Value>) -> Self {
        Self {
            key,
            target,
            arguments,
        }
    }
}

/// What a runner port gives for one call: the answer of the child, or its
/// expected failure. An error of the port itself is the call's `Err`. More
/// replies may come in a later version: a `match` on it outside the crate
/// needs a wildcard arm.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RunnerReply {
    /// The child answered.
    Answered(RunnerAnswer),
    /// The child gave no answer: it refused, failed, was cancelled by someone
    /// else, or did not answer in time. A runner call node fails with
    /// `GraphError::RunnerFailed`; a runner tool gives an error result
    /// holding `reason`.
    Failed {
        /// Why the child gave no answer.
        reason: String,
    },
}

/// The port through which a runner call reaches another runner, the host's:
/// the library owns the node, the tool and the call key, and the port does
/// everything else.
///
/// What a port does with one call (`RunnerRequest`):
/// - it derives from the call's key whatever it creates, the child's thread
///   and job among them, so the same key finds the call it had started
///   instead of starting another (see `CallKey`);
/// - it waits for the child, and gives back its answer
///   (`RunnerReply::Answered`: the blocks of its reply in order, and its
///   thread), its expected failure with a reason (`RunnerReply::Failed`: the
///   child refused, failed, was cancelled by someone else, or did not answer
///   in time), or an error of its own (`Err`);
/// - it reads the answer back, and finishes or cancels the child's job.
///
/// The library awaits the call and owns no task, no timer and no transport:
/// a deadline on the answer is the port's.
///
/// A cancel of the run drops a running call: its future is dropped at the
/// await it stands at, so a port's call must be safe to drop there. The port
/// cancels the child of a call dropped before it ends, once, and never the
/// child of a call whose end it already settled: a guard armed when the
/// child is created, and disarmed once the call has decided the child's
/// fate, does both.
///
/// What a resumed run asks of the port, which it must handle:
/// - a runner call node that finished in the interrupted superstep, or a map
///   item that finished, is recorded and not run again; one that did not
///   finish, or whose node failed, calls the port again under its key;
/// - inside a called graph nothing is recorded: the call restarts whole, so
///   its runner calls that had finished are made again under their keys, and
///   the port finds them and gives the ends it had;
/// - a runner tool of a durable agent whose call a crash interrupted is
///   answered as interrupted when the agent enters its thread again, and is
///   never awaited again: the port's deadline or its own cleanup settles that
///   call's child;
/// - a runner tool of an agent in memory restarts with the agent: the model
///   is asked again and its new tool calls give their own ids, so the keys
///   follow the model's new ids.
pub trait RunnerPort: Send + Sync {
    /// Makes the call `request`, or finds the one its key names, and waits
    /// for its end.
    fn call<'a>(&'a self, request: &'a RunnerRequest) -> RunnerFuture<'a>;
}

/// The error `error` of a runner port as a runner call node fails with it:
/// `Runner`, carrying it. A function, not a `From`: `RunnerError` is any
/// boxed error, so a conversion would turn the error of a node, a tool or a
/// hook into `Runner` as well.
pub(crate) fn runner_failed(error: RunnerError) -> crate::error::GraphError {
    crate::error::GraphError::Runner { source: error }
}
