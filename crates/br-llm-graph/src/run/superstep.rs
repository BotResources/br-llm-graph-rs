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

/// A node of the superstep, with the context it runs in.
struct StepNode<'a> {
    id: &'a NodeId,
    node: &'a dyn Node,
    ctx: Context,
}

/// Runs the nodes of `active` together. The observer hears each node start
/// before any runs and, once they have all finished, each node finish with
/// how it ended; a cancel that drops them while they run sends no finish.
/// Both events and the results come from one list: the nodes of `active` the
/// graph holds, all of them, since the cursor is checked and an edge to an
/// unknown node is refused.
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
    let nodes: Vec<StepNode<'_>> = active
        .iter()
        .filter_map(|id| {
            graph.node(id).map(|node| StepNode {
                id,
                node,
                ctx: ctx.for_node(id),
            })
        })
        .collect();
    for step_node in &nodes {
        ctx.observer.node_started(ctx.origin(), step_node.id);
    }
    let seen = nodes.len() > 1;
    let futures = nodes
        .iter()
        .map(|step_node| run_recorded(step_node.node, state, config, &step_node.ctx, seen));
    let superstep = join_all(futures);
    futures_util::pin_mut!(superstep);
    let mut channel_open = true;
    let result = loop {
        if !channel_open {
            let raw = superstep.await;
            break done(ctx, &nodes, raw);
        }
        match select(superstep, control.receiver.next()).await {
            Either::Left((raw, _pending_message)) => {
                break done(ctx, &nodes, raw);
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

/// The results of a completed superstep, in the order of its nodes; the
/// observer hears each node finish first.
fn done(ctx: &Context, nodes: &[StepNode<'_>], raw: Vec<RawOutcome>) -> StepResult {
    let mut results = Vec::with_capacity(raw.len());
    for (step_node, outcome) in nodes.iter().zip(raw) {
        let ended = node_outcome(&outcome);
        ctx.observer
            .node_finished(ctx.origin(), step_node.id, ended);
        results.push((step_node.id.clone(), node_result(outcome)));
    }
    StepResult::Done(results)
}

/// How a node of the superstep ended: with its updates, returned or
/// recorded, with its error or with its panic. A refusal of its updates
/// comes later, when the superstep is settled.
fn node_outcome(outcome: &RawOutcome) -> NodeOutcome {
    match outcome {
        Ok(Ok(_)) => NodeOutcome::Updates,
        Ok(Err(_)) => NodeOutcome::Error,
        Err(_) => NodeOutcome::Panic,
    }
}

fn node_result(outcome: RawOutcome) -> Result<Vec<Update>, NodeFault> {
    match outcome {
        Ok(Ok(updates)) => Ok(updates),
        Ok(Err(error)) => Err(NodeFault::Returned(error)),
        Err(payload) => Err(NodeFault::Panic(panic_message(payload))),
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
