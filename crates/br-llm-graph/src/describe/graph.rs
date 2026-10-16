//! A built graph's description of itself.

use crate::describe::{DescribedNode, GraphDescription};
use crate::graph::Graph;

impl Graph {
    /// What the graph says of itself as data: its entry, its signature, its
    /// schema (its state and configuration keys with their kinds and
    /// defaults; an agent's private keys are not part of it), and its nodes
    /// in registration order, each with its kind, its realizes labels, its
    /// edge's declared targets (in the order given, or undeclared) and the
    /// fields of its kind, nested graphs included.
    ///
    /// It runs nothing, calls no client, reads no state and no
    /// configuration, and is the same for the same graph: two builds from
    /// the same parts give equal descriptions, in the same order.
    ///
    /// ```
    /// use br_llm_graph::{Always, EndLabel, FnNode, GraphBuilder, Node, NodeFuture, NodeId, Schema, Target};
    ///
    /// let id = NodeId::new("draft").unwrap();
    /// let draft = FnNode::new(|_: &_, _: &_, _: &_| -> NodeFuture<'_> {
    ///     Box::pin(async { Ok(Vec::new()) })
    /// });
    /// let graph = GraphBuilder::new(Schema::builder().build())
    ///     .entry(id.clone())
    ///     .node(id.clone(), draft.realizing(["DraftNote"]))
    ///     .edge(id, Always(Target::End(EndLabel::new("done").unwrap())))
    ///     .build()
    ///     .unwrap();
    /// let json = serde_json::to_value(graph.describe()).unwrap();
    /// assert_eq!(json["description_version"], "br-llm-graph-description/1");
    /// assert_eq!(
    ///     json["nodes"][0],
    ///     serde_json::json!({
    ///         "id": "draft",
    ///         "kind": "node",
    ///         "realizes": ["DraftNote"],
    ///         "edge": {"targets": [{"end": "done"}]},
    ///     })
    /// );
    /// ```
    pub fn describe(&self) -> GraphDescription {
        let nodes = self
            .order()
            .iter()
            .filter_map(|id| {
                self.node(id).map(|node| DescribedNode {
                    id: id.clone(),
                    is_join: self.is_join(id),
                    edge: self.edge(id).and_then(|entry| entry.declared.clone()),
                    node: node.describe(),
                })
            })
            .collect();
        GraphDescription {
            entry: self.entry().clone(),
            signature: self.signature().clone(),
            schema: self.schema().clone(),
            nodes,
        }
    }
}
