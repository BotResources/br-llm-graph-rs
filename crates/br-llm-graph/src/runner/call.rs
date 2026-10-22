//! A runner call as a node of a graph: it asks another runner for an answer
//! through the runner port, waits for it and gives it back as data.

use std::sync::Arc;

use crate::describe::NodeDescription;
use crate::describe::shape::{RunnerCallShape, Shape};
use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture, Output};
use crate::react::check_text_output;
use crate::runner::arguments::RunnerArguments;
use crate::runner::port::{CallKey, RunnerPort, RunnerReply, RunnerRequest, runner_failed};
use crate::runner::target::RunnerTarget;
use crate::state::{Config, Schema, State, StateKey, Value};
use crate::update::Update;
use crate::value::Key;

/// Where a runner call writes a text it gives (its answer, its child's
/// thread): a string state key it sets, or a list of strings it appends to.
///
/// More outputs may come in a later version: a `match` on it outside the
/// crate needs a wildcard arm.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunnerOutput {
    /// Sets a string state key to the text.
    Set(StateKey<String>),
    /// Appends the text to a list of strings.
    Append(StateKey<Vec<String>>),
}

impl RunnerOutput {
    /// The key the text goes to.
    pub fn key(&self) -> &Key {
        match self {
            RunnerOutput::Set(key) => key.key(),
            RunnerOutput::Append(key) => key.key(),
        }
    }

    /// Where it writes, by name, as a description writes it.
    pub(crate) fn written(&self) -> Output {
        match self {
            RunnerOutput::Set(key) => Output::Set(key.key().clone()),
            RunnerOutput::Append(key) => Output::Append(key.key().clone()),
        }
    }

    fn update(&self, text: String) -> Update {
        self.written().update(Value::str(text))
    }
}

/// A node that asks another runner for an answer through the runner port,
/// waits for it and gives it back as data.
///
/// Each run makes one call: its key is the node's origin, the run id and the
/// node's occurrence (see `CallKey`), its target `target`, its arguments
/// those `arguments` give for the state and the configuration. The port
/// does the rest: the child's thread and job, the wait, a deadline when
/// there is one, the read-back (see `RunnerPort`). An occurrence does not
/// tell the supersteps of a run apart: a node a loop of its graph runs again
/// in one run calls under the same key, and the port finds the call it had
/// made.
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
/// failure of a node, its reason the error's message. An error of the port
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
/// At build, the node checks its outputs, refused with the error writing a
/// string through them would give (`UnknownKey`, `KindMismatch`,
/// `AppendNotList`).
pub struct RunnerCall {
    /// The runner type of the runner it calls.
    pub target: RunnerTarget,
    /// What the child is given, a JSON object.
    pub arguments: RunnerArguments,
    /// The port the call goes through, the host's.
    pub port: Arc<dyn RunnerPort>,
    /// Where the answer's text goes.
    pub output: RunnerOutput,
    /// Where the id of the child's thread goes. `None`: nowhere.
    pub thread: Option<RunnerOutput>,
}

impl Node for RunnerCall {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let request = RunnerRequest::new(
                CallKey::new(ctx.origin().clone(), None),
                self.target.clone(),
                self.arguments.give(state, config)?,
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
        check_text_output(schema, &self.output.written(), site)?;
        match &self.thread {
            Some(thread) => check_text_output(schema, &thread.written(), site),
            None => Ok(()),
        }
    }

    /// `runner_call`: its target, how its arguments are given, never their
    /// value, and its outputs.
    fn describe(&self) -> NodeDescription {
        NodeDescription::of(Shape::RunnerCall(RunnerCallShape {
            target: self.target.as_str().to_owned(),
            arguments: self.arguments.origin(),
            output: self.output.written(),
            thread: self.thread.as_ref().map(RunnerOutput::written),
        }))
    }
}
