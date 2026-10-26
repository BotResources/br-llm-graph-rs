use std::collections::HashSet;
use std::sync::Arc;

use br_graph_contract::{Fields, Graph, State};

use crate::run::Run;
use crate::{Outcome, RunConfig, StartError};

pub async fn start<F: Fields>(
    graph: Arc<Graph<F>>,
    state: State<F>,
    nodes: Vec<String>,
    config: RunConfig,
) -> Result<Outcome<F>, StartError> {
    check_nodes(&graph, &nodes)?;
    let run = Run::new(graph, Arc::new(state), &nodes, config);
    Ok(run.drive(nodes, None).await)
}

fn check_nodes<F: Fields>(graph: &Graph<F>, nodes: &[String]) -> Result<(), StartError> {
    if nodes.is_empty() {
        return Err(StartError::NoNodes);
    }
    let mut seen = HashSet::new();
    for name in nodes {
        if !seen.insert(name.as_str()) {
            return Err(StartError::DuplicateNode { name: name.clone() });
        }
        if graph.node(name).is_none() {
            return Err(StartError::UnknownNode { name: name.clone() });
        }
    }
    Ok(())
}
