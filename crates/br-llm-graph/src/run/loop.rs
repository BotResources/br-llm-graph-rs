// Ok and Err arms both carry State; boxing the Err alone saves nothing.
#![allow(clippy::result_large_err)]

use br_llm_messages::UserInput;

use crate::error::GraphError;
use crate::graph::{Context, Graph};
use crate::run::checkpoint::Checkpoint;
use crate::run::control::{Command, Control};
use crate::run::cursor::Cursor;
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::settle::settle;
use crate::run::superstep::{StepResult, drive_superstep};
use crate::state::{Config, State};
use crate::update::Update;
use crate::value::NodeId;

/// Runs `graph` from `state` (and `cursor`, when resuming) until it ends,
/// pauses, is cancelled or fails. A state or a configuration not made from
/// the graph's schema fails the run before anything runs
/// (`StateSchemaMismatch`, `ConfigSchemaMismatch`). A cursor naming a node
/// the graph does not hold fails the run (`UnknownNode`), an empty cursor too
/// (`EmptyCursor`).
///
/// The run ends when no node is left to run, with the end label the edges of
/// that final superstep returned (several distinct labels fail it with
/// `AmbiguousEnd`). An end label returned in a superstep that still leaves
/// nodes to run, while other branches carry on, is dropped.
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
///
/// A graph with private keys, an agent's graph and its `conversation`,
/// records nothing either: no state it gives back holds its private keys,
/// and its checkpoints name its entry, through which a resumed run enters
/// again.
pub async fn run(
    graph: &Graph,
    config: &Config,
    state: State,
    cursor: Option<Cursor>,
    ctx: &Context,
    control: &mut Control,
) -> Result<Outcome, RunFailure> {
    // A graph with private keys is resumed through its entry: its runs
    // record nothing and reuse nothing (see `graph/private.rs`).
    let ctx = if graph.has_private() {
        ctx.unrecorded()
    } else {
        ctx.isolated()
    };
    if let Err(error) = graph.check_schemas(&state, config) {
        let cursor = cursor.unwrap_or_default();
        return Err(fail(
            graph,
            state,
            &ctx,
            &cursor.active,
            &cursor.deferred,
            error,
        ));
    }
    run_nested(graph, config, state, cursor, &ctx, control).await
}

/// Runs `graph` on the recorder of `ctx`, shared with the caller: a nested
/// run records under the caller's occurrence, into the caller's checkpoint.
///
/// The run works on the state with the graph's private keys, and takes them
/// out of every state it gives back (see `graph/private.rs`). It has run
/// slots of its own, dropped when it returns, however it ends.
pub(crate) async fn run_nested(
    graph: &Graph,
    config: &Config,
    state: State,
    cursor: Option<Cursor>,
    ctx: &Context,
    control: &mut Control,
) -> Result<Outcome, RunFailure> {
    let ctx = &ctx.for_run();
    let (mut active, mut deferred) = match cursor {
        Some(cursor) if cursor.is_empty() => {
            return Err(fail(graph, state, ctx, &[], &[], GraphError::EmptyCursor));
        }
        Some(cursor) => (cursor.active, cursor.deferred),
        None => (vec![graph.entry().clone()], Vec::new()),
    };
    if let Err(error) = graph.validate_cursor(&Cursor::new(active.clone(), deferred.clone())) {
        return Err(fail(graph, state, ctx, &active, &deferred, error));
    }
    let mut state = graph.enter_private(state);

    loop {
        for id in &active {
            ctx.observer.node_started(ctx.origin(), id);
        }
        let step = drive_superstep(graph, &state, config, ctx, &active, control).await;
        let mut held_inputs: Vec<(_, UserInput)> = step.held_inputs;
        let mut pause = step.pause;
        let results = match step.result {
            StepResult::Cancelled => {
                return cancelled(graph, state, ctx, held_inputs, &active, &deferred);
            }
            StepResult::Done(results) => results,
        };
        for id in &active {
            ctx.observer.node_finished(ctx.origin(), id);
        }

        if drain_after_step(control, &mut held_inputs, &mut pause) {
            return cancelled(graph, state, ctx, held_inputs, &active, &deferred);
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
                return Err(fail(graph, state, ctx, &active, &deferred, error));
            }
        };
        state = settled.state;
        for update in &settled.applied {
            ctx.observer.applied(ctx.origin(), update);
        }
        ctx.drop_pending(&active);

        let cursor = Cursor::new(settled.active.clone(), settled.deferred.clone());
        observe_checkpoint(graph, ctx, &state, &cursor);

        if let Some(end) = settled.end {
            ctx.observer.run_finished(ctx.origin(), &end);
            let state = graph.leave_private(state);
            return Ok(Outcome::Finished { state, end });
        }
        if pause {
            let checkpoint = checkpoint(graph, state, ctx, cursor);
            return Ok(Outcome::Paused { checkpoint });
        }
        active = settled.active;
        deferred = settled.deferred;
    }
}

fn drain_after_step(
    control: &mut Control,
    held_inputs: &mut Vec<(crate::value::Key, UserInput)>,
    pause: &mut bool,
) -> bool {
    while let Some(message) = control.try_next() {
        match message {
            Command::Input { key, input } => held_inputs.push((key, input)),
            Command::Pause => *pause = true,
            Command::Resume => {}
            Command::Cancel => return true,
        }
    }
    false
}

/// The observer's `checkpoint` event after a superstep: the state without
/// the graph's private keys, and the cursor a checkpoint would name.
fn observe_checkpoint(graph: &Graph, ctx: &Context, state: &State, cursor: &Cursor) {
    if graph.has_private() {
        let state = graph.leave_private(state.clone());
        let cursor = graph.checkpoint_cursor(cursor.clone());
        ctx.observer.checkpoint(ctx.origin(), &state, &cursor);
    } else {
        ctx.observer.checkpoint(ctx.origin(), state, cursor);
    }
}

/// The checkpoint of a run standing at `cursor` on `state`, with the pending
/// entries of the run: without the graph's private keys, at the entry for a
/// graph that has some.
fn checkpoint(graph: &Graph, state: State, ctx: &Context, cursor: Cursor) -> Checkpoint {
    Checkpoint::new(graph.leave_private(state), graph.checkpoint_cursor(cursor))
        .with_pending(ctx.pending_here())
}

fn cancelled(
    graph: &Graph,
    mut state: State,
    ctx: &Context,
    held_inputs: Vec<(crate::value::Key, UserInput)>,
    active: &[NodeId],
    deferred: &[NodeId],
) -> Result<Outcome, RunFailure> {
    let inputs = input_updates(held_inputs);
    if let Err(error) = state.apply_batch(&inputs) {
        return Err(fail(graph, state, ctx, active, deferred, error));
    }
    for update in &inputs {
        ctx.observer.applied(ctx.origin(), update);
    }
    let cursor = Cursor::new(active.to_vec(), deferred.to_vec());
    let checkpoint = checkpoint(graph, state, ctx, cursor);
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
    graph: &Graph,
    state: State,
    ctx: &Context,
    active: &[NodeId],
    deferred: &[NodeId],
    error: GraphError,
) -> RunFailure {
    let cursor = Cursor::new(active.to_vec(), deferred.to_vec());
    RunFailure {
        checkpoint: checkpoint(graph, state, ctx, cursor),
        error,
    }
}
