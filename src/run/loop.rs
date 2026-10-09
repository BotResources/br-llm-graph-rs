// Ok and Err arms both carry State; boxing the Err alone saves nothing.
#![allow(clippy::result_large_err)]

use br_llm_messages::UserInput;

use crate::error::GraphError;
use crate::graph::{Context, Graph};
use crate::run::checkpoint::Checkpoint;
use crate::run::cursor::Cursor;
use crate::run::inbox::{Inbox, Message};
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::settle::settle;
use crate::run::superstep::{StepResult, drive_superstep};
use crate::state::{Config, State};
use crate::update::Update;
use crate::value::NodeId;

/// Runs `graph` from `state` (and `cursor`, when resuming) until it ends,
/// pauses, is cancelled or fails.
///
/// A superstep runs its nodes together and is applied as a whole: when one of
/// them fails, its updates are refused, or the edges or the end cannot be
/// resolved, nothing of the superstep reaches the state, and the failure
/// checkpoint holds the state from before it with the cursor of the whole
/// superstep. Each node that finished recorded its updates under its
/// occurrence (a pending entry), so a resume skips it and runs only the others.
///
/// The run keeps pending entries in a recorder of its own, seeded with those
/// `ctx` holds: to resume a checkpoint, pass its state, its cursor and
/// `ctx.with_pending(checkpoint.pending)`. The checkpoint of a failure, a
/// cancel or a pause carries the pending entries of the run; those of a
/// superstep are dropped once it is applied. Pending entries belong to the run
/// being resumed: its own nodes and the items of its maps (a map directly in
/// a map included). A called graph restarts whole, so nothing inside it is
/// recorded or reused.
pub async fn run(
    graph: &Graph,
    config: &Config,
    state: State,
    cursor: Option<Cursor>,
    ctx: &Context,
    inbox: &mut Inbox,
) -> Result<Outcome, RunFailure> {
    run_nested(graph, config, state, cursor, &ctx.isolated(), inbox).await
}

/// Runs `graph` on the recorder of `ctx`, shared with the caller: a nested
/// run records under the caller's occurrence, into the caller's checkpoint.
pub(crate) async fn run_nested(
    graph: &Graph,
    config: &Config,
    mut state: State,
    cursor: Option<Cursor>,
    ctx: &Context,
    inbox: &mut Inbox,
) -> Result<Outcome, RunFailure> {
    let (mut active, mut deferred) = match cursor {
        Some(cursor) => (cursor.active, cursor.deferred),
        None => (vec![graph.entry().clone()], Vec::new()),
    };
    if let Err(error) = graph.validate_cursor(&Cursor::new(active.clone(), deferred.clone())) {
        return Err(fail(state, ctx, &active, &deferred, error));
    }

    loop {
        for id in &active {
            ctx.observer.node_started(ctx.origin(), id);
        }
        let step = drive_superstep(graph, &state, config, ctx, &active, inbox).await;
        let mut held_inputs: Vec<(_, UserInput)> = step.held_inputs;
        let mut pause = step.pause;
        let results = match step.result {
            StepResult::Cancelled => {
                return cancelled(state, ctx, held_inputs, &active, &deferred);
            }
            StepResult::Done(results) => results,
        };
        for id in &active {
            ctx.observer.node_finished(ctx.origin(), id);
        }

        if drain_after_step(inbox, &mut held_inputs, &mut pause) {
            return cancelled(state, ctx, held_inputs, &active, &deferred);
        }

        let inputs = input_updates(held_inputs);
        let settled = match settle(
            graph,
            config,
            &state,
            results,
            &inputs,
            (&active, &deferred),
            ctx,
        ) {
            Ok(settled) => settled,
            Err(error) => {
                // Nothing of the superstep reaches the state; the held inputs
                // do, as they would have on any other outcome.
                let state = with_inputs(state, ctx, &inputs);
                return Err(fail(state, ctx, &active, &deferred, error));
            }
        };
        state = settled.state;
        for update in &settled.applied {
            ctx.observer.applied(ctx.origin(), update);
        }
        ctx.drop_pending(&active);

        let cursor = Cursor::new(settled.active.clone(), settled.deferred.clone());
        ctx.observer.checkpoint(ctx.origin(), &state, &cursor);

        if let Some(end) = settled.end {
            ctx.observer.run_finished(ctx.origin(), &end);
            return Ok(Outcome::Finished { state, end });
        }
        if pause {
            let checkpoint = Checkpoint::new(state, cursor).with_pending(ctx.pending_here());
            return Ok(Outcome::Paused { checkpoint });
        }
        active = settled.active;
        deferred = settled.deferred;
    }
}

fn drain_after_step(
    inbox: &mut Inbox,
    held_inputs: &mut Vec<(crate::value::Key, UserInput)>,
    pause: &mut bool,
) -> bool {
    while let Some(message) = inbox.try_next() {
        match message {
            Message::Input { key, input } => held_inputs.push((key, input)),
            Message::Pause => *pause = true,
            Message::Resume => {}
            Message::Cancel => return true,
        }
    }
    false
}

fn cancelled(
    mut state: State,
    ctx: &Context,
    held_inputs: Vec<(crate::value::Key, UserInput)>,
    active: &[NodeId],
    deferred: &[NodeId],
) -> Result<Outcome, RunFailure> {
    let inputs = input_updates(held_inputs);
    if let Err(error) = state.apply_batch(&inputs) {
        return Err(fail(state, ctx, active, deferred, error));
    }
    for update in &inputs {
        ctx.observer.applied(ctx.origin(), update);
    }
    let checkpoint = Checkpoint::new(state, Cursor::new(active.to_vec(), deferred.to_vec()))
        .with_pending(ctx.pending_here());
    Ok(Outcome::Cancelled { checkpoint })
}

fn input_updates(held_inputs: Vec<(crate::value::Key, UserInput)>) -> Vec<Update> {
    held_inputs
        .into_iter()
        .map(|(key, input)| Update::Input { key, input })
        .collect()
}

/// `state` with the held inputs, when it takes them; unchanged otherwise.
fn with_inputs(mut state: State, ctx: &Context, inputs: &[Update]) -> State {
    if !inputs.is_empty() && state.apply_batch(inputs).is_ok() {
        for update in inputs {
            ctx.observer.applied(ctx.origin(), update);
        }
    }
    state
}

fn fail(
    state: State,
    ctx: &Context,
    active: &[NodeId],
    deferred: &[NodeId],
    error: GraphError,
) -> RunFailure {
    let cursor = Cursor::new(active.to_vec(), deferred.to_vec());
    RunFailure {
        checkpoint: Checkpoint::new(state, cursor).with_pending(ctx.pending_here()),
        error,
    }
}
