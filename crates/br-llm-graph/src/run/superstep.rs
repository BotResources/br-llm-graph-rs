use std::any::Any;
use std::panic::AssertUnwindSafe;

use br_llm_messages::UserInput;
use futures_util::future::{Either, join_all, select};
use futures_util::{FutureExt, StreamExt};

use crate::error::NodeFault;
use crate::graph::{Context, Graph, Node, NodeError};
use crate::observe::NodeOutcome;
use crate::run::control::{Command, Control};
use crate::state::{Config, State};
use crate::update::Update;
use crate::value::{Key, NodeId};

type RawOutcome = Result<Result<Vec<Update>, NodeError>, Box<dyn Any + Send>>;

pub(crate) enum StepResult {
    Done(Vec<(NodeId, Result<Vec<Update>, NodeFault>)>),
    Cancelled,
}

pub(crate) struct SuperstepOutput {
    pub result: StepResult,
    pub held_inputs: Vec<(Key, UserInput)>,
    pub pause: bool,
}

pub(crate) async fn drive_superstep(
    graph: &Graph,
    state: &State,
    config: &Config,
    ctx: &Context,
    active: &[NodeId],
    control: &mut Control,
) -> SuperstepOutput {
    let mut held_inputs: Vec<(Key, UserInput)> = Vec::new();
    let mut pause = false;
    let present: Vec<NodeId> = active
        .iter()
        .filter(|id| graph.node(id).is_some())
        .cloned()
        .collect();
    let contexts: Vec<Context> = present.iter().map(|id| ctx.for_node(id)).collect();
    let seen = present.len() > 1;
    let futures = present.iter().zip(&contexts).filter_map(|(id, node_ctx)| {
        graph
            .node(id)
            .map(|node| run_recorded(node, state, config, node_ctx, seen))
    });
    let superstep = join_all(futures);
    futures_util::pin_mut!(superstep);
    let mut channel_open = true;
    let result = loop {
        if !channel_open {
            let raw = superstep.await;
            break StepResult::Done(pair(present, raw));
        }
        match select(superstep, control.receiver.next()).await {
            Either::Left((raw, _pending_message)) => {
                break StepResult::Done(pair(present, raw));
            }
            Either::Right((message, remaining)) => {
                superstep = remaining;
                match message {
                    None => channel_open = false,
                    Some(Command::Input { key, input }) => held_inputs.push((key, input)),
                    Some(Command::Pause) => pause = true,
                    Some(Command::Resume) => {}
                    Some(Command::Cancel) => break StepResult::Cancelled,
                }
            }
        }
    };
    SuperstepOutput {
        result,
        held_inputs,
        pause,
    }
}

/// Runs a node of the superstep in its own occurrence. A node an earlier
/// attempt recorded is not run: its recorded updates are used. A node that
/// returns updates records them as soon as it finishes, so a failure or a
/// cancel of the superstep keeps them; an error or a panic records nothing.
/// A panic is caught whether it happens while `run` starts (before it returns
/// its future) or inside the future.
/// The observer sees the record only when the node has siblings (`seen`).
async fn run_recorded(
    node: &dyn Node,
    state: &State,
    config: &Config,
    ctx: &Context,
    seen: bool,
) -> RawOutcome {
    if let Some(updates) = ctx.recorded() {
        return Ok(Ok(updates));
    }
    let started = std::panic::catch_unwind(AssertUnwindSafe(|| node.run(state, config, ctx)));
    let outcome = match started {
        Ok(future) => AssertUnwindSafe(future).catch_unwind().await,
        Err(payload) => Err(payload),
    };
    if let Ok(Ok(updates)) = &outcome {
        if seen {
            ctx.record(updates);
        } else {
            ctx.record_unseen(updates);
        }
    }
    outcome
}

fn pair(
    present: Vec<NodeId>,
    raw: Vec<RawOutcome>,
) -> Vec<(NodeId, Result<Vec<Update>, NodeFault>)> {
    present
        .into_iter()
        .zip(raw)
        .map(|(id, outcome)| {
            let result = match outcome {
                Ok(Ok(updates)) => Ok(updates),
                Ok(Err(error)) => Err(NodeFault::Returned(error)),
                Err(payload) => Err(NodeFault::Panic(panic_message(payload))),
            };
            (id, result)
        })
        .collect()
}

/// How a node of the superstep ended, from its result. Its updates are
/// refused only once the superstep is settled: such a node returned them.
pub(crate) fn node_outcome(result: &Result<Vec<Update>, NodeFault>) -> NodeOutcome {
    match result {
        Ok(_) | Err(NodeFault::Refused(_)) => NodeOutcome::Updates,
        Err(NodeFault::Returned(_)) => NodeOutcome::Error,
        Err(NodeFault::Panic(_)) => NodeOutcome::Panic,
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a node panicked".to_owned()
    }
}
