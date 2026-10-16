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
//! A description writes no state value, no default, no prompt, no client,
//! no author name and no secret: nothing a function computes and no tool
//! argument is part of it. A subject is written as how it was given
//! (`fixed`, `ready_made`, `function`); a fixed value of a call or a
//! capture, and a state key's default, as its kind; an author by its id,
//! never its name, which is rendered into what a model reads. It does write
//! the graph's fixed bounds: a fixed `Limit` or `Count` (`{"fixed": n}`)
//! and the generator-critic's `final_generation`.

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
/// It is written, never reloaded: it implements `Serialize` only. Its JSON
/// is `{"description_version", "entry", "signature", "schema", "nodes"}`.
/// `signature` is in its own JSON shape. `schema` is `{"state", "config",
/// "defaults"?}`: `state` and `config` map each key to its kind, and
/// `defaults` maps each state key that has a default to its kind, never the
/// default's value (`"defaults": {"persona": {"type": "str"}}`); an agent's
/// private keys are not part of it. A nested graph is the same object
/// without `description_version`.
///
/// A node of `nodes` is `{"id", "kind", "realizes"?, "join"?, "edge", ...}`,
/// its fields in that order, then the fields of its kind:
///
/// - `realizes`: its realizes labels, in the order given; omitted when it
///   has none.
/// - `join`: `true` for a node registered as a join whose kind is not
///   `join`; omitted otherwise.
/// - `edge`: `{"targets": [...]}` with `{"node": id}` and `{"end": label}`
///   items, in the order declared, each once; or `{"undeclared": true}`.
///
/// A field marked `?` is omitted when empty or absent. The fields of each
/// kind, in the order written:
///
/// | Kind | Fields |
/// |---|---|
/// | `node`, `join` | none; a host's node may give its own kind label instead, and `"graph"?`, the description of a graph it runs |
/// | `call` | `"inputs"?`, `"config"?`: `[{"child", "source"}]`, the source `{"from": key}`, `{"config": key}` or `{"const": kind}`; `"outputs"?`: `[{"child", "target"}]`, the target `{"set": key}` or `{"append": key}`; `"end_label"?`: a target; `"on_failure"`: `"propagate"` or `{"capture": [{"target", "source"}]}`, a capture's source also `{"reason": true}`; `"graph"`: the graph it runs |
/// | `react_agent`, `generator_critic` | a call of an agent's graph: the fields of `call` |
/// | `map` | `"list"`, `"item"`, `"max_concurrency"?`, `"on_item_failure"`: `"finish"`, `"fail_fast"` or `{"capture": [...]}`; `"body"`: its body, a node without `id`, `join` and `edge` |
/// | `llm_call` | `"model"`, `"system"`; `"request"`: `{"text": subject}`, `{"conversation": key, "author": {"id"}}` or `{"history": subject}`; `"output"`: a target |
/// | `model_call` | `"author"`: `{"id"}`, the author's id, never its name; `"model"`, `"system"`, `"history"`; `"tools"?`: tool names; `"enabled"?`; `"tool_concurrency"?`; `"round_limit"?`: `{"max_rounds", "on_limit"}`, `on_limit` `"error"`, `{"continue": {"node", "flag"}}` or `{"end": {"node", "flag"}}`. The critic's adds `"max_critiques"`, `"final_generation"`, `"verdict_retries"`, `"review_request"`, `"verdict"` (`"default"` or `"override"`) |
/// | `tools` | `"tools"`: the names of the tools it runs |
///
/// A subject (`model`, `system`, `history`, a request's text...) is written
/// `"fixed"`, `"ready_made"` or `"function"`, never its value; a `Limit` or
/// a `Count` `{"fixed": n}` or `{"config": key}`. The generator-critic's
/// `check` is a `node` holding `"revision_limit"`, `"rule_check"?` and
/// `"empty_answer_critique"`. `graph` nests a whole graph (a call's, a host
/// node's); `body` is one node (a map's).
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

#[cfg(test)]
mod describe_call_tests;
#[cfg(test)]
mod describe_map_tests;
#[cfg(test)]
mod describe_tests;
#[cfg(test)]
mod realizing_tests;
#[cfg(test)]
pub(crate) mod test_support;
