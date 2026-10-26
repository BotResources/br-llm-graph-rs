use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_graph_contract::{Fields, Graph, State};

use crate::run::Run;
use crate::{Checkpoint, Outcome, ResumeError, RunConfig};

pub async fn resume<F: Fields>(
    graph: Arc<Graph<F>>,
    checkpoint: Checkpoint,
    limit: NonZeroUsize,
    config: RunConfig,
) -> Result<Outcome<F>, ResumeError> {
    check_nodes(&graph, &checkpoint)?;
    check_structure(&graph, &checkpoint)?;
    let state = State::<F>::rebuild_with_limit(checkpoint.state_json().as_bytes(), limit).map_err(
        |error| ResumeError::StateRejected {
            reason: error.to_string(),
        },
    )?;
    let nodes = checkpoint.nodes().to_vec();
    let run = Run::new(graph, Arc::new(state), &nodes, config);
    Ok(run.drive(Vec::new(), Some(checkpoint)).await)
}

fn check_nodes<F: Fields>(graph: &Graph<F>, checkpoint: &Checkpoint) -> Result<(), ResumeError> {
    if checkpoint.nodes().is_empty() {
        return Err(ResumeError::NoNodes);
    }
    let mut seen = HashSet::new();
    for name in checkpoint.nodes() {
        if !seen.insert(name.as_str()) {
            return Err(ResumeError::DuplicateNode { name: name.clone() });
        }
        if graph.node(name).is_none() {
            return Err(ResumeError::UnknownNode { name: name.clone() });
        }
    }
    Ok(())
}

fn check_structure<F: Fields>(
    graph: &Graph<F>,
    checkpoint: &Checkpoint,
) -> Result<(), ResumeError> {
    let wanted = graph.structure();
    let given = checkpoint.structure();
    if given.version() != wanted.version() {
        return Err(ResumeError::VersionMismatch {
            checkpoint: given.version().to_owned(),
            graph: wanted.version().to_owned(),
        });
    }
    let given_names = given.nodes().iter().map(|node| &node.name);
    let wanted_names = wanted.nodes().iter().map(|node| &node.name);
    if !given_names.eq(wanted_names) {
        return Err(ResumeError::StructureMismatch {
            reason: "the node names differ".to_owned(),
        });
    }
    if given.edges() != wanted.edges() {
        return Err(ResumeError::StructureMismatch {
            reason: "the edges differ".to_owned(),
        });
    }
    Ok(())
}
