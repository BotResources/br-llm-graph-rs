// Ok and Err arms both carry State; boxing the Err alone saves nothing.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use br_llm_messages::UserInput;

use crate::error::{GraphError, NodeFault};
use crate::graph::{Context, Graph, Target};
use crate::run::checkpoint::Checkpoint;
use crate::run::cursor::Cursor;
use crate::run::inbox::{Inbox, Message};
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::superstep::{StepResult, drive_superstep};
use crate::state::{Config, State};
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

pub async fn run(
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
        return Err(fail(state, &active, &deferred, error));
    }

    loop {
        for id in &active {
            ctx.observer.node_started(id);
        }
        let step = drive_superstep(graph, &state, config, ctx, &active, inbox).await;
        let mut held_inputs: Vec<(_, UserInput)> = step.held_inputs;
        let mut pause = step.pause;
        let results = match step.result {
            StepResult::Cancelled => {
                let checkpoint = Checkpoint::new(state, Cursor::new(active, deferred));
                return Ok(Outcome::Cancelled { checkpoint });
            }
            StepResult::Done(results) => results,
        };
        for id in &active {
            ctx.observer.node_finished(id);
        }

        if let Some(outcome) = drain_after_step(
            inbox,
            &mut held_inputs,
            &mut pause,
            &state,
            &active,
            &deferred,
        ) {
            return outcome;
        }

        let mut by_id: BTreeMap<NodeId, Result<Vec<Update>, NodeFault>> =
            results.into_iter().collect();
        let mut failure: Option<(NodeId, NodeFault)> = None;
        for id in graph.order() {
            let Some(result) = by_id.remove(id) else {
                continue;
            };
            match result {
                Ok(updates) => match state.apply_batch(&updates) {
                    Ok(()) => {
                        for update in &updates {
                            ctx.observer.applied(update);
                        }
                    }
                    Err(error) => {
                        if failure.is_none() {
                            failure = Some((id.clone(), NodeFault::Refused(Box::new(error))));
                        }
                    }
                },
                Err(fault) => {
                    if failure.is_none() {
                        failure = Some((id.clone(), fault));
                    }
                }
            }
        }
        if let Some((node, source)) = failure {
            let error = GraphError::NodeFailed { node, source };
            return Err(fail(state, &active, &deferred, error));
        }

        if let Err(error) = apply_inputs(&mut state, ctx, held_inputs) {
            return Err(fail(state, &active, &deferred, error));
        }

        let (produced, ends) = match evaluate_edges(graph, config, &state, &active) {
            Ok(pair) => pair,
            Err(error) => return Err(fail(state, &active, &deferred, error)),
        };

        let (active_next, deferred_next) = next_sets(graph, produced, &deferred);
        let cursor = Cursor::new(active_next.clone(), deferred_next.clone());
        ctx.observer.checkpoint(&state, &cursor);

        if active_next.is_empty() && deferred_next.is_empty() {
            return finish(state, &active, &deferred, ends, ctx);
        }
        if pause {
            let checkpoint = Checkpoint::new(state, cursor);
            return Ok(Outcome::Paused { checkpoint });
        }
        active = active_next;
        deferred = deferred_next;
    }
}

fn drain_after_step(
    inbox: &mut Inbox,
    held_inputs: &mut Vec<(crate::value::Key, UserInput)>,
    pause: &mut bool,
    state: &State,
    active: &[NodeId],
    deferred: &[NodeId],
) -> Option<Result<Outcome, RunFailure>> {
    while let Some(message) = inbox.try_next() {
        match message {
            Message::Input { key, input } => held_inputs.push((key, input)),
            Message::Pause => *pause = true,
            Message::Resume => {}
            Message::Cancel => {
                let checkpoint = Checkpoint::new(
                    state.clone(),
                    Cursor::new(active.to_vec(), deferred.to_vec()),
                );
                return Some(Ok(Outcome::Cancelled { checkpoint }));
            }
        }
    }
    None
}

fn apply_inputs(
    state: &mut State,
    ctx: &Context,
    held_inputs: Vec<(crate::value::Key, UserInput)>,
) -> Result<(), GraphError> {
    if held_inputs.is_empty() {
        return Ok(());
    }
    let updates: Vec<Update> = held_inputs
        .into_iter()
        .map(|(key, input)| Update::Input { key, input })
        .collect();
    state.apply_batch(&updates)?;
    for update in &updates {
        ctx.observer.applied(update);
    }
    Ok(())
}

fn evaluate_edges(
    graph: &Graph,
    config: &Config,
    state: &State,
    active: &[NodeId],
) -> Result<(Vec<NodeId>, Vec<EndLabel>), GraphError> {
    let mut produced: Vec<NodeId> = Vec::new();
    let mut ends: Vec<EndLabel> = Vec::new();
    for id in graph.order() {
        if !active.contains(id) {
            continue;
        }
        let edge = graph
            .edge(id)
            .ok_or_else(|| GraphError::NodeWithoutEdge { id: id.clone() })?;
        let targets = edge.next(state, config)?;
        if targets.is_empty() {
            return Err(GraphError::EmptyEdge { node: id.clone() });
        }
        for target in targets {
            match target {
                Target::Node(node) => {
                    if !produced.contains(&node) {
                        produced.push(node);
                    }
                }
                Target::End(label) => ends.push(label),
            }
        }
    }
    Ok((produced, ends))
}

fn next_sets(
    graph: &Graph,
    produced: Vec<NodeId>,
    deferred: &[NodeId],
) -> (Vec<NodeId>, Vec<NodeId>) {
    let mut pending: Vec<NodeId> = Vec::new();
    for id in produced.into_iter().chain(deferred.iter().cloned()) {
        if !pending.contains(&id) {
            pending.push(id);
        }
    }
    let mut joins: Vec<NodeId> = Vec::new();
    let mut plain: Vec<NodeId> = Vec::new();
    for id in pending {
        if graph.is_join(&id) {
            joins.push(id);
        } else {
            plain.push(id);
        }
    }
    if plain.is_empty() {
        (joins, Vec::new())
    } else {
        (plain, joins)
    }
}

fn finish(
    state: State,
    active: &[NodeId],
    deferred: &[NodeId],
    ends: Vec<EndLabel>,
    ctx: &Context,
) -> Result<Outcome, RunFailure> {
    let mut distinct: Vec<EndLabel> = Vec::new();
    for label in ends {
        if !distinct.contains(&label) {
            distinct.push(label);
        }
    }
    let mut iter = distinct.into_iter();
    match (iter.next(), iter.next()) {
        (Some(end), None) => {
            ctx.observer.run_finished(&end);
            Ok(Outcome::Finished { state, end })
        }
        (first, second) => {
            let mut labels: Vec<EndLabel> = first.into_iter().chain(second).collect();
            labels.extend(iter);
            Err(fail(
                state,
                active,
                deferred,
                GraphError::AmbiguousEnd { labels },
            ))
        }
    }
}

fn fail(state: State, active: &[NodeId], deferred: &[NodeId], error: GraphError) -> RunFailure {
    RunFailure {
        checkpoint: Checkpoint::new(state, Cursor::new(active.to_vec(), deferred.to_vec())),
        error,
    }
}
