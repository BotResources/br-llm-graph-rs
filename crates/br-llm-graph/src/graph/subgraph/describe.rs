//! What a call says of itself in a description: its mappings, its failure
//! policy and the description of the graph it runs.

use crate::describe::NodeDescription;
use crate::describe::shape::{CallShape, CaptureShape, Shape, Source};
use crate::graph::subgraph::{CaptureSource, CaptureUpdate, Input, OnFailure, SubGraph};
use crate::state::{Kind, Value};
use crate::value::Key;

impl SubGraph {
    /// The description of the call: `call`, or the kind of the agent whose
    /// graph it runs. A fixed value is written as the kind of the child key
    /// it fills, never the value.
    pub(crate) fn description(&self) -> NodeDescription {
        let child = &self.graph;
        let mapped = |mappings: &[(Key, Input)], kinds: &std::collections::BTreeMap<Key, Kind>| {
            mappings
                .iter()
                .map(|(key, input)| (key.clone(), source(input, kinds.get(key))))
                .collect()
        };
        NodeDescription::of(Shape::Call(Box::new(CallShape {
            agent: child.agent(),
            inputs: mapped(&self.inputs, &child.signature().inputs),
            config: mapped(&self.config, &child.schema().config),
            outputs: self.outputs.clone(),
            end_label: self.end_label.clone(),
            captures: match &self.on_failure {
                OnFailure::Propagate => None,
                OnFailure::Capture(captures) => Some(capture_shapes(captures)),
            },
            graph: child.describe(),
        })))
    }
}

fn source(input: &Input, kind: Option<&Kind>) -> Source {
    match input {
        Input::From(key) => Source::From(key.clone()),
        Input::Config(key) => Source::Config(key.clone()),
        Input::Const(value) => Source::Const(kind.cloned().or_else(|| kind_of(value))),
    }
}

/// The capture updates of a call or a map, each fixed value written as its
/// kind.
pub(crate) fn capture_shapes(captures: &[CaptureUpdate]) -> Vec<CaptureShape> {
    captures
        .iter()
        .map(|capture| CaptureShape {
            target: capture.target(),
            source: match capture.source() {
                CaptureSource::Const(value) => Source::Const(kind_of(value)),
                CaptureSource::From(key) => Source::From(key.clone()),
                CaptureSource::Reason => Source::Reason,
            },
        })
        .collect()
}

/// The kind of `value`; `None` for an empty list, or a list whose elements
/// have no kind in common.
fn kind_of(value: &Value) -> Option<Kind> {
    match value {
        Value::Int(_) => Some(Kind::Int),
        Value::Float(_) => Some(Kind::Float),
        Value::Str(_) => Some(Kind::Str),
        Value::Bool(_) => Some(Kind::Bool),
        Value::Conversation(_) => Some(Kind::Conversation),
        Value::List(items) => {
            let kind = Kind::list(kind_of(items.first()?)?);
            value.matches(&kind).then_some(kind)
        }
    }
}
