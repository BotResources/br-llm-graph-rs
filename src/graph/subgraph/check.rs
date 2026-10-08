use std::collections::BTreeSet;

use crate::error::GraphError;
use crate::graph::subgraph::{Input, Output, SubGraph};
use crate::state::{Kind, Schema};
use crate::value::{Key, NodeId};

/// Where a call is placed: a node of the graph, or the body of a map, whose
/// updates may only be appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Placement {
    Node,
    MapBody,
}

/// Checks a call against the schema of the graph that holds it.
pub(crate) fn check_call(
    schema: &Schema,
    node: &NodeId,
    call: &SubGraph,
    placement: Placement,
) -> Result<(), GraphError> {
    check_inputs(schema, node, call)?;
    check_config(schema, node, call)?;
    let outputs = &call.graph.signature().outputs;
    for (child_key, target) in &call.outputs {
        let kind = outputs
            .get(child_key)
            .ok_or_else(|| GraphError::SubGraphNotAnOutput {
                node: node.clone(),
                key: child_key.clone(),
            })?;
        check_target(schema, node, kind, target, placement)?;
    }
    if let Some(target) = &call.end_label {
        check_target(schema, node, &Kind::Str, target, placement)?;
    }
    Ok(())
}

fn check_inputs(schema: &Schema, node: &NodeId, call: &SubGraph) -> Result<(), GraphError> {
    let declared = &call.graph.signature().inputs;
    let mut mapped: BTreeSet<&Key> = BTreeSet::new();
    for (child_key, source) in &call.inputs {
        let kind = declared
            .get(child_key)
            .ok_or_else(|| GraphError::SubGraphNotAnInput {
                node: node.clone(),
                key: child_key.clone(),
            })?;
        if !mapped.insert(child_key) {
            return Err(GraphError::SubGraphInputTwice {
                node: node.clone(),
                key: child_key.clone(),
            });
        }
        if !source_fits(schema, source, kind) {
            return Err(GraphError::SubGraphSourceMismatch {
                node: node.clone(),
                key: child_key.clone(),
            });
        }
    }
    match declared.keys().find(|key| !mapped.contains(key)) {
        Some(key) => Err(GraphError::SubGraphInputUnmapped {
            node: node.clone(),
            key: key.clone(),
        }),
        None => Ok(()),
    }
}

fn check_config(schema: &Schema, node: &NodeId, call: &SubGraph) -> Result<(), GraphError> {
    let declared = &call.graph.schema().config;
    let mut mapped: BTreeSet<&Key> = BTreeSet::new();
    for (child_key, source) in &call.config {
        let kind = declared
            .get(child_key)
            .ok_or_else(|| GraphError::SubGraphNotAConfig {
                node: node.clone(),
                key: child_key.clone(),
            })?;
        if !mapped.insert(child_key) {
            return Err(GraphError::SubGraphConfigTwice {
                node: node.clone(),
                key: child_key.clone(),
            });
        }
        if !source_fits(schema, source, kind) {
            return Err(GraphError::SubGraphConfigMismatch {
                node: node.clone(),
                key: child_key.clone(),
            });
        }
    }
    match declared.keys().find(|key| !mapped.contains(key)) {
        Some(key) => Err(GraphError::SubGraphConfigUnmapped {
            node: node.clone(),
            key: key.clone(),
        }),
        None => Ok(()),
    }
}

fn source_fits(schema: &Schema, source: &Input, kind: &Kind) -> bool {
    match source {
        Input::From(key) => schema.state.get(key) == Some(kind),
        Input::Config(key) => schema.config.get(key) == Some(kind),
        Input::Const(value) => value.matches(kind),
    }
}

/// A target receives a value of `kind`: a `Set` key of that kind, or an
/// `Append` list of that kind. In a map body only appends are allowed.
fn check_target(
    schema: &Schema,
    node: &NodeId,
    kind: &Kind,
    target: &Output,
    placement: Placement,
) -> Result<(), GraphError> {
    let fits = match target {
        Output::Set(key) => {
            if placement == Placement::MapBody {
                return Err(GraphError::MapBodySet {
                    node: node.clone(),
                    key: key.clone(),
                });
            }
            schema.state.get(key) == Some(kind)
        }
        Output::Append(key) => match schema.state.get(key) {
            Some(Kind::List { element }) => element.as_ref() == kind,
            Some(_) | None => false,
        },
    };
    if fits {
        Ok(())
    } else {
        Err(GraphError::SubGraphTargetMismatch {
            node: node.clone(),
            key: target.key().clone(),
        })
    }
}
