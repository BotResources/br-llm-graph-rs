use std::any::Any;
use std::panic::AssertUnwindSafe;

use br_llm_messages::UserInput;
use futures_util::future::{Either, join_all, select};
use futures_util::{FutureExt, StreamExt};

use crate::error::NodeFault;
use crate::graph::{Context, Graph, Node, NodeError};
use crate::run::inbox::{Inbox, Message};
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
    inbox: &mut Inbox,
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
        match select(superstep, inbox.receiver.next()).await {
            Either::Left((raw, _pending_message)) => {
                break StepResult::Done(pair(present, raw));
            }
            Either::Right((message, remaining)) => {
                superstep = remaining;
                match message {
                    None => channel_open = false,
                    Some(Message::Input { key, input }) => held_inputs.push((key, input)),
                    Some(Message::Pause) => pause = true,
                    Some(Message::Resume) => {}
                    Some(Message::Cancel) => break StepResult::Cancelled,
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
    let outcome = AssertUnwindSafe(node.run(state, config, ctx))
        .catch_unwind()
        .await;
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

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_owned()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "a node panicked".to_owned()
    }
}
