//! What a built graph says of itself as data, without running anything: its
//! entry, its signature, its schema, and its nodes in registration order,
//! each with its kind, its realizes labels, its edge's declared targets and
//! the fields of its kind, nested graphs included.
//!
//! `Graph::describe` gives a `GraphDescription`; its JSON (`serde`) is the
//! description, versioned `br-llm-graph-description/1`
//! (`DESCRIPTION_VERSION`). It is written, never reloaded: no graph is built
//! from one. A node says what it is through `Node::describe`, a host's node
//! by default `node` or `join` as registered, with nothing nested; it may give
//! its own kind label and a nested description (`NodeDescription::labelled`,
//! `NodeDescription::nesting`). Realizes labels name what a node implements
//! and the library never reads them (`Node::realizing`).
//!
//! Nothing a function computes, no prompt text, no client, no tool argument
//! and no secret is part of a description: a subject is written as how it
//! was given (`fixed`, `ready_made`, `function`), a fixed value of a call or
//! a capture as its kind.

mod graph;
mod json;
mod json_fields;
mod json_values;
mod realizing;
pub(crate) mod shape;

use crate::error::GraphError;
use crate::graph::{Signature, Target};
use crate::state::Schema;
use crate::value::{NodeId, is_identifier};

pub use realizing::Realizing;
use shape::{LIBRARY_KINDS, Shape};

/// The version a description is written under.
pub const DESCRIPTION_VERSION: &str = "br-llm-graph-description/1";

/// What a built graph says of itself (`Graph::describe`). Two builds from
/// the same parts give equal descriptions, in the same order.
///
/// Its JSON is `{"description_version", "entry", "signature", "schema",
/// "nodes"}`; a node is `{"id", "kind", "realizes"?, "join"?, "edge", ...}`
/// with the fields of its kind, an edge `{"targets": [{"node"} | {"end"}]}`
/// or `{"undeclared": true}`, a nested graph a description without its
/// version. It is written, never reloaded: it implements `Serialize` only.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphDescription {
    pub(crate) entry: NodeId,
    pub(crate) signature: Signature,
    pub(crate) schema: Schema,
    pub(crate) nodes: Vec<DescribedNode>,
}

/// One node of a described graph: its id, how it is registered, its edge's
/// declared targets (`None`: undeclared) and what it says of itself.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DescribedNode {
    pub(crate) id: NodeId,
    pub(crate) is_join: bool,
    pub(crate) edge: Option<Vec<Target>>,
    pub(crate) node: NodeDescription,
}

/// What a node says of itself in its graph's description: its kind and the
/// fields of its kind, and its realizes labels. The graph adds its id and its
/// edge.
///
/// By default (`NodeDescription::default`) a node has no kind of its own:
/// it is written `node`, or `join` when registered as one, with nothing
/// nested and no label. The library's own nodes give their kinds (`call`,
/// `react_agent`, `generator_critic`, `map`, `llm_call`, `model_call`,
/// `tools`); a host's node may give its own kind label and its own nested
/// description.
///
/// ```
/// use br_llm_graph::{GraphError, NodeDescription};
///
/// let router = NodeDescription::labelled("router").unwrap().realizing(["RouteRequests"]);
/// assert_ne!(router, NodeDescription::default());
/// assert!(matches!(
///     NodeDescription::labelled("map"),
///     Err(GraphError::Identifier { field: "kind_label", .. })
/// ));
/// ```
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NodeDescription {
    pub(crate) shape: Shape,
    pub(crate) realizes: Vec<String>,
}

impl NodeDescription {
    /// A host's node of its own kind, `label`, written as its kind. A label
    /// that does not match `[a-z][a-z0-9_]*`, or that names one of the kinds
    /// the library writes (`node`, `join`, `call`, `map`, `llm_call`,
    /// `react_agent`, `generator_critic`, `tools`, `model_call`), is refused
    /// (`Identifier`, the field `kind_label`).
    pub fn labelled(label: impl Into<String>) -> Result<Self, GraphError> {
        let label = label.into();
        if !is_identifier(&label) || LIBRARY_KINDS.contains(&label.as_str()) {
            return Err(GraphError::Identifier {
                field: "kind_label",
                value: label,
            });
        }
        Ok(Self::of(Shape::Host {
            label: Some(label),
            nested: None,
        }))
    }

    /// This description of a host's node with `graph` as its nested
    /// description, written as its field `graph`: a graph the node runs. The
    /// description of one of the library's own kinds already holds what it
    /// nests, and is left unchanged.
    pub fn nesting(mut self, graph: GraphDescription) -> Self {
        if let Shape::Host { nested, .. } = &mut self.shape {
            *nested = Some(Box::new(graph));
        }
        self
    }

    /// This description with `labels` added after its realizes labels, in
    /// the order given. Labels are free text, never read by the library.
    pub fn realizing<L: Into<String>>(mut self, labels: impl IntoIterator<Item = L>) -> Self {
        self.realizes.extend(labels.into_iter().map(Into::into));
        self
    }

    /// The description of one of the library's own shapes.
    pub(crate) fn of(shape: Shape) -> Self {
        Self {
            shape,
            realizes: Vec::new(),
        }
    }
}
