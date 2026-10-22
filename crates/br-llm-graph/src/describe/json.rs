//! The JSON of a description, `br-llm-graph-description/1`: the graph, its
//! nodes and their edges. Written only: nothing here deserializes.

use std::collections::BTreeMap;

use serde::ser::{Serialize, SerializeMap, Serializer};

use crate::describe::json_fields::{
    call_fields, check_fields, llm_call_fields, map_fields, model_call_fields, runner_call_fields,
};
use crate::describe::shape::Shape;
use crate::describe::{DESCRIPTION_VERSION, DescribedNode, GraphDescription, NodeDescription};
use crate::graph::Target;
use crate::state::{Kind, Schema};
use crate::value::{EndLabel, Key, NodeId};

impl Serialize for GraphDescription {
    /// `{"description_version", "entry", "signature", "schema", "nodes"}`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("description_version", DESCRIPTION_VERSION)?;
        graph_fields(self, &mut map)?;
        map.end()
    }
}

/// A nested graph: a description without its own version.
pub(crate) struct Nested<'a>(pub(crate) &'a GraphDescription);

impl Serialize for Nested<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        graph_fields(self.0, &mut map)?;
        map.end()
    }
}

fn graph_fields<M: SerializeMap>(graph: &GraphDescription, map: &mut M) -> Result<(), M::Error> {
    map.serialize_entry("entry", &graph.entry)?;
    map.serialize_entry("signature", &graph.signature)?;
    map.serialize_entry("schema", &SchemaJson(&graph.schema))?;
    map.serialize_entry("nodes", &graph.nodes)
}

/// A schema as a description writes it: `{"state", "config", "defaults"?}`,
/// `defaults` mapping each key that has a default to the key's kind, never
/// the default's value, which may hold prompt text or a secret.
struct SchemaJson<'a>(&'a Schema);

impl Serialize for SchemaJson<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let schema = self.0;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("state", &schema.state)?;
        map.serialize_entry("config", &schema.config)?;
        let defaults: BTreeMap<&Key, &Kind> = schema
            .defaults
            .keys()
            .filter_map(|key| schema.state.get(key).map(|kind| (key, kind)))
            .collect();
        if !defaults.is_empty() {
            map.serialize_entry("defaults", &defaults)?;
        }
        map.end()
    }
}

impl Serialize for DescribedNode {
    /// `{"id", "kind", "realizes"?, "join"?, "edge", ...}`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("kind", self.node.shape.kind(self.is_join))?;
        realizes(&self.node, &mut map)?;
        if self.is_join && self.node.shape.marks_join() {
            map.serialize_entry("join", &true)?;
        }
        match &self.edge {
            Some(targets) => map.serialize_entry("edge", &Declared { targets })?,
            None => map.serialize_entry("edge", &Undeclared { undeclared: true })?,
        }
        shape_fields(&self.node.shape, &mut map)?;
        map.end()
    }
}

impl Serialize for NodeDescription {
    /// A node without its id and its edge, as a map's body is written:
    /// `{"kind", "realizes"?, ...}`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", self.shape.kind(false))?;
        realizes(self, &mut map)?;
        shape_fields(&self.shape, &mut map)?;
        map.end()
    }
}

fn realizes<M: SerializeMap>(node: &NodeDescription, map: &mut M) -> Result<(), M::Error> {
    if node.realizes.is_empty() {
        return Ok(());
    }
    map.serialize_entry("realizes", &node.realizes)
}

#[derive(serde::Serialize)]
struct Declared<'a> {
    targets: &'a [Target],
}

#[derive(serde::Serialize)]
struct Undeclared {
    undeclared: bool,
}

impl Serialize for Target {
    /// `{"node": id}` or `{"end": label}`.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "snake_case")]
        enum Json<'a> {
            Node(&'a NodeId),
            End(&'a EndLabel),
        }
        match self {
            Target::Node(id) => Json::Node(id),
            Target::End(label) => Json::End(label),
        }
        .serialize(serializer)
    }
}

fn shape_fields<M: SerializeMap>(shape: &Shape, map: &mut M) -> Result<(), M::Error> {
    match shape {
        Shape::Host { nested, .. } => match nested {
            Some(graph) => map.serialize_entry("graph", &Nested(graph)),
            None => Ok(()),
        },
        Shape::Call(call) => call_fields(call, map),
        Shape::Map(shape) => map_fields(shape, map),
        Shape::LlmCall(call) => llm_call_fields(call, map),
        Shape::ModelCall(call) => model_call_fields(call, map),
        Shape::Tools(tools) => map.serialize_entry("tools", &tools.tools),
        Shape::Check(check) => check_fields(check, map),
        Shape::RunnerCall(call) => runner_call_fields(call, map),
    }
}
