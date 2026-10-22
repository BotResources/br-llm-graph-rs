//! A runner call as a node of a graph: it asks another runner for an answer
//! through the runner port, waits for it and gives it back as data.

use std::sync::Arc;

use crate::describe::NodeDescription;
use crate::describe::shape::{RunnerCallShape, Shape};
use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture, TextOutput};
use crate::runner::arguments::RunnerArguments;
use crate::runner::key::CallKey;
use crate::runner::port::{RunnerPort, RunnerReply, RunnerRequest, runner_failed};
use crate::runner::target::RunnerTarget;
use crate::state::{Config, Schema, State};

/// A node that asks another runner for an answer through the runner port,
/// waits for it and gives it back as data.
///
/// Each run makes one call: its arguments those `arguments` give for the
/// state and the configuration, its target `target`, its key the node's
/// origin (the run id and the node's occurrence) and its arguments (see
/// `CallKey`). The port does the rest: the child's thread and job, the wait,
/// a deadline when there is one, the read-back (see `RunnerPort`). A node
/// that a loop of its graph runs again with the same arguments names the
/// same call and gets the same answer; with other arguments, it makes
/// another call.
///
/// The answer's text (the JSON text of its structured block when it has
/// one, else its text blocks joined verbatim) is written through `output`,
/// and the id of the child's thread through `thread` when set. Nothing else
/// is written.
///
/// An expected failure of the child (it refused, failed, was cancelled by
/// someone else, or did not answer in time) fails the node with
/// `RunnerFailed`, naming the target and carrying the reason: the `Capture`
/// of a call or of a map holding the node turns it into data, as for any
/// failure of a node. Its message is the child's reason as it gave it
/// (`runner call failed` when it is blank), so a map's capture of the reason
/// records it verbatim (a call's records the failure of its graph, which
/// names the node). An error of the port
/// fails the node with `Runner`, carrying it; an error of an arguments
/// function fails it as returned. A cancel of the run drops the call under
/// way, whose child the port cancels.
///
/// As the body of a `Map`, an `Append` output makes one call per item, under
/// the item's own key (`m[3]`), at most the map's concurrency limit at once,
/// the answers appended in item order; a `Set` output, of the answer or of
/// the thread, is refused there (`MapBodySet`). A resumed map calls the
/// port only for the items that did not finish, under the keys they had.
///
/// A description writes it as `runner_call`: its target, how its arguments
/// are given (fixed or a function), never their value, and its outputs.
///
/// At build, the node checks its outputs, refused with the error writing a
/// string through them would give (`UnknownKey`, `KindMismatch`,
/// `AppendNotList`), and an answer and a thread that `Set` one key
/// (`SetConflict`, which their one batch would meet once the child ran).
pub struct RunnerCall {
    /// The runner type of the runner it calls.
    pub target: RunnerTarget,
    /// What the child is given, a JSON object.
    pub arguments: RunnerArguments,
    /// The port the call goes through, the host's.
    pub port: Arc<dyn RunnerPort>,
    /// Where the answer's text goes.
    pub output: TextOutput,
    /// Where the id of the child's thread goes. `None`: nowhere.
    pub thread: Option<TextOutput>,
}

impl Node for RunnerCall {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let arguments = self.arguments.give(state, config)?;
            let request = RunnerRequest::new(
                CallKey::new(ctx.origin().clone(), &arguments, None),
                self.target.clone(),
                arguments,
            );
            let answer = match self.port.call(&request).await {
                Ok(RunnerReply::Answered(answer)) => answer,
                Ok(RunnerReply::Failed { reason }) => {
                    return Err(GraphError::RunnerFailed {
                        target: self.target.clone(),
                        reason,
                    }
                    .into());
                }
                Err(error) => return Err(runner_failed(error).into()),
            };
            let mut updates = vec![self.output.update(answer.text())];
            if let Some(thread) = &self.thread {
                updates.push(thread.update(answer.thread.to_string()));
            }
            Ok(updates)
        })
    }

    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        self.output.check(schema, site)?;
        let Some(thread) = &self.thread else {
            return Ok(());
        };
        thread.check(schema, site)?;
        // Both in one batch: two `Set` of one key would be refused after the
        // child ran.
        match (&self.output, thread) {
            (TextOutput::Set(answer), TextOutput::Set(thread)) if answer == thread => {
                Err(GraphError::SetConflict {
                    key: answer.key().clone(),
                })
            }
            _ => Ok(()),
        }
    }

    /// `runner_call`: its target, how its arguments are given, never their
    /// value, and its outputs.
    fn describe(&self) -> NodeDescription {
        NodeDescription::of(Shape::RunnerCall(RunnerCallShape {
            target: self.target.as_str().to_owned(),
            arguments: self.arguments.how_given(),
            output: self.output.written(),
            thread: self.thread.as_ref().map(TextOutput::written),
        }))
    }
}
