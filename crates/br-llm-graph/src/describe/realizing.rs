//! A node carrying realizes labels: the names of what it implements, which
//! the library writes in the description and never reads.

use crate::describe::NodeDescription;
use crate::error::GraphError;
use crate::graph::{CheckSite, Context, Node, NodeFuture};
use crate::state::{Config, Schema, State};

/// A node with realizes labels (`Node::realizing`). It runs, checks and
/// describes itself as its node does, and adds its labels after the labels
/// its node gives. It is registered like any node (`GraphBuilder::node`,
/// `GraphBuilder::join`), and may be the body of a map.
///
/// ```
/// use br_llm_graph::{Config, Context, FnNode, Node, NodeFuture, State};
///
/// let draft = FnNode::new(|_: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
///     Box::pin(async { Ok(Vec::new()) })
/// })
/// .realizing(["DraftNote"]);
/// assert_eq!(draft.labels(), ["DraftNote"]);
/// ```
pub struct Realizing<N> {
    node: N,
    labels: Vec<String>,
}

impl<N> Realizing<N> {
    pub(crate) fn new<L: Into<String>>(node: N, labels: impl IntoIterator<Item = L>) -> Self {
        Self {
            node,
            labels: labels.into_iter().map(Into::into).collect(),
        }
    }

    /// Its realizes labels, in the order given.
    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    /// The node it carries.
    pub fn node(&self) -> &N {
        &self.node
    }
}

impl<N: Node> Node for Realizing<N> {
    fn run<'a>(&'a self, state: &'a State, config: &'a Config, ctx: &'a Context) -> NodeFuture<'a> {
        self.node.run(state, config, ctx)
    }

    fn check(&self, schema: &Schema, site: CheckSite) -> Result<(), GraphError> {
        self.node.check(schema, site)
    }

    fn describe(&self) -> NodeDescription {
        self.node.describe().realizing(self.labels.iter().cloned())
    }
}
