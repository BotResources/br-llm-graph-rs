use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use br_graph_contract::{Fields, Graph, State};

use crate::panic_text::panic_reason;
use crate::run::{Origin, Run};
use crate::{Checkpoint, Outcome, ResumeError, RunConfig, node_names};

pub async fn resume<F: Fields>(
    graph: Arc<Graph<F>>,
    checkpoint: Checkpoint,
    limit: NonZeroUsize,
    config: RunConfig,
) -> Result<Outcome<F>, ResumeError> {
    node_names::check(&graph, checkpoint.nodes())?;
    check_structure(&graph, &checkpoint)?;
    let state = rebuild_state::<F>(&checkpoint, limit)?;
    let run = Run::new(graph, Arc::new(state), config);
    Ok(run.drive(Origin::Checkpoint(checkpoint)).await)
}

fn rebuild_state<F: Fields>(
    checkpoint: &Checkpoint,
    limit: NonZeroUsize,
) -> Result<State<F>, ResumeError> {
    let rebuilt = catch_unwind(AssertUnwindSafe(|| {
        State::<F>::rebuild_with_limit(checkpoint.state_json().as_bytes(), limit)
    }));
    match rebuilt {
        Ok(Ok(state)) => Ok(state),
        Ok(Err(error)) => Err(ResumeError::StateRejected {
            reason: error.to_string(),
        }),
        Err(payload) => Err(ResumeError::StateRejected {
            reason: panic_reason(&*payload),
        }),
    }
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
