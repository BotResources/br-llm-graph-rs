use std::collections::BTreeSet;

use crate::error::GraphError;
use crate::graph::subgraph::{CaptureSource, CaptureUpdate, Input, OnFailure, Output, SubGraph};
use crate::state::{Kind, Schema};
use crate::value::Key;

/// Checks a call against the schema of the graph it runs in.
pub(crate) fn check_call(schema: &Schema, call: &SubGraph) -> Result<(), GraphError> {
    check_inputs(schema, call)?;
    check_config(schema, call)?;
    let outputs = &call.graph.signature().outputs;
    for (child_key, target) in &call.outputs {
        let kind = outputs
            .get(child_key)
            .ok_or_else(|| GraphError::SubGraphNotAnOutput {
                key: child_key.clone(),
            })?;
        check_target(schema, kind, target)?;
    }
    if let Some(target) = &call.end_label {
        check_target(schema, &Kind::Str, target)?;
    }
    if let OnFailure::Capture(captures) = &call.on_failure {
        for capture in captures {
            check_capture(schema, capture, false)?;
        }
    }
    Ok(())
}

fn check_inputs(schema: &Schema, call: &SubGraph) -> Result<(), GraphError> {
    let declared = &call.graph.signature().inputs;
    let mut mapped: BTreeSet<&Key> = BTreeSet::new();
    for (child_key, source) in &call.inputs {
        let kind = declared
            .get(child_key)
            .ok_or_else(|| GraphError::SubGraphNotAnInput {
                key: child_key.clone(),
            })?;
        if !mapped.insert(child_key) {
            return Err(GraphError::SubGraphInputTwice {
                key: child_key.clone(),
            });
        }
        if !source_fits(schema, source, kind) {
            return Err(GraphError::SubGraphSourceMismatch {
                key: child_key.clone(),
            });
        }
    }
    match declared.keys().find(|key| !mapped.contains(key)) {
        Some(key) => Err(GraphError::SubGraphInputUnmapped { key: key.clone() }),
        None => Ok(()),
    }
}

fn check_config(schema: &Schema, call: &SubGraph) -> Result<(), GraphError> {
    let declared = &call.graph.schema().config;
    let mut mapped: BTreeSet<&Key> = BTreeSet::new();
    for (child_key, source) in &call.config {
        let kind = declared
            .get(child_key)
            .ok_or_else(|| GraphError::SubGraphNotAConfig {
                key: child_key.clone(),
            })?;
        if !mapped.insert(child_key) {
            return Err(GraphError::SubGraphConfigTwice {
                key: child_key.clone(),
            });
        }
        if !source_fits(schema, source, kind) {
            return Err(GraphError::SubGraphConfigMismatch {
                key: child_key.clone(),
            });
        }
    }
    match declared.keys().find(|key| !mapped.contains(key)) {
        Some(key) => Err(GraphError::SubGraphConfigUnmapped { key: key.clone() }),
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
/// `Append` list of that kind.
fn check_target(schema: &Schema, kind: &Kind, target: &Output) -> Result<(), GraphError> {
    if receives(schema, target) == Some(kind) {
        Ok(())
    } else {
        Err(GraphError::SubGraphTargetMismatch {
            key: target.key().clone(),
        })
    }
}

/// The kind a target takes: the kind of a `Set` key, the element kind of an
/// `Append` list.
fn receives<'a>(schema: &'a Schema, target: &Output) -> Option<&'a Kind> {
    match target {
        Output::Set(key) => schema.state.get(key),
        Output::Append(key) => match schema.state.get(key) {
            Some(Kind::List { element }) => Some(element.as_ref()),
            Some(_) | None => None,
        },
    }
}

/// A capture update fits its target. With `appends_only` (a map's own
/// captures) a `Set` is refused.
pub(crate) fn check_capture(
    schema: &Schema,
    capture: &CaptureUpdate,
    appends_only: bool,
) -> Result<(), GraphError> {
    let target = capture.target();
    if let (Output::Set(key), true) = (&target, appends_only) {
        return Err(GraphError::MapBodySet { key: key.clone() });
    }
    let fits = match (receives(schema, &target), capture.source()) {
        (None, _) => false,
        (Some(kind), CaptureSource::Const(value)) => value.matches(kind),
        (Some(kind), CaptureSource::From(key)) => schema.state.get(key) == Some(kind),
        (Some(kind), CaptureSource::Reason) => *kind == Kind::Str,
    };
    if fits {
        Ok(())
    } else {
        Err(GraphError::CaptureMismatch {
            key: target.key().clone(),
        })
    }
}
