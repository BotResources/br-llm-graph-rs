use std::sync::Arc;

use br_graph_contract::{Fields, Graph, State};

use crate::run::{Origin, Run};
use crate::{Outcome, RunConfig, StartError, node_names};

pub async fn start<F: Fields>(
    graph: Arc<Graph<F>>,
    state: State<F>,
    nodes: Vec<String>,
    config: RunConfig,
) -> Result<Outcome<F>, StartError> {
    node_names::check(&graph, &nodes)?;
    let run = Run::new(graph, Arc::new(state), Origin::Nodes(nodes), config);
    Ok(run.drive().await)
}
