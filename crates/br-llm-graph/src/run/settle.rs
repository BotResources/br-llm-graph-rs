//! What a finished superstep leads to, computed on a copy of the state: the
//! run loop commits it only when every step of it succeeds.

use std::collections::BTreeMap;

use crate::error::{GraphError, NodeFault};
use crate::graph::{Context, Graph, Target};
use crate::state::{Config, State};
use crate::update::Update;
use crate::value::{EndLabel, NodeId};

pub(crate) struct Settled {
    pub state: State,
    /// Every update applied, node batches in graph order then inputs.
    pub applied: Vec<Update>,
    pub active: Vec<NodeId>,
    pub deferred: Vec<NodeId>,
    /// The label the run ends with, when nothing is left to run.
    pub end: Option<EndLabel>,
}

/// Applies the results of `active`'s nodes and the held inputs to a copy of
/// `state`, evaluates the edges and, when nothing is left to run, the end
/// label. Any failure leaves `state` as it was. The pending entry of a node
/// whose updates are refused is forgotten, so a resume runs it again.
pub(crate) fn settle(
    graph: &Graph,
    config: &Config,
    state: &State,
    results: Vec<(NodeId, Result<Vec<Update>, NodeFault>)>,
    inputs: &[Update],
    (active, deferred): (&[NodeId], &[NodeId]),
    ctx: &Context,
) -> Result<Settled, GraphError> {
    let mut next = state.clone();
    let mut applied: Vec<Update> = Vec::new();
    let mut by_id: BTreeMap<NodeId, Result<Vec<Update>, NodeFault>> = results.into_iter().collect();
    let mut failure: Option<(NodeId, NodeFault)> = None;
    for id in graph.order() {
        let Some(result) = by_id.remove(id) else {
            continue;
        };
        let fault = match result {
            Ok(updates) => match next.apply_batch(&updates) {
                Ok(()) => {
                    applied.extend(updates);
                    continue;
                }
                Err(error) => {
                    ctx.forget_node(id);
                    NodeFault::Refused(Box::new(error))
                }
            },
            Err(fault) => fault,
        };
        if failure.is_none() {
            failure = Some((id.clone(), fault));
        }
    }
    if let Some((node, source)) = failure {
        return Err(GraphError::NodeFailed { node, source });
    }
    next.apply_batch(inputs)?;
    applied.extend(inputs.iter().cloned());

    let (produced, ends) = evaluate_edges(graph, config, &next, active)?;
    let (active, deferred) = next_sets(graph, produced, deferred);
    let end = if active.is_empty() && deferred.is_empty() {
        Some(single_end(ends)?)
    } else {
        None
    };
    Ok(Settled {
        state: next,
        applied,
        active,
        deferred,
        end,
    })
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
                    if !graph.contains(&node) {
                        return Err(GraphError::UnknownNode { id: node });
                    }
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

fn single_end(ends: Vec<EndLabel>) -> Result<EndLabel, GraphError> {
    let mut distinct: Vec<EndLabel> = Vec::new();
    for label in ends {
        if !distinct.contains(&label) {
            distinct.push(label);
        }
    }
    let mut iter = distinct.into_iter();
    match (iter.next(), iter.next()) {
        (Some(end), None) => Ok(end),
        (first, second) => {
            let mut labels: Vec<EndLabel> = first.into_iter().chain(second).collect();
            labels.extend(iter);
            Err(GraphError::AmbiguousEnd { labels })
        }
    }
}
