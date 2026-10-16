//! A built graph's description: what it holds, that it runs nothing and is
//! the same for the same graph, and the JSON of its nodes and edges.

use serde_json::json;

use super::test_support::*;
use crate::describe::{DESCRIPTION_VERSION, NodeDescription};
use crate::error::GraphError;
use crate::graph::{Always, FnEdge, Graph, GraphBuilder, Node};
use crate::state::{Config, Kind, Schema, State, Value};

fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("topic"), Kind::Str)
        .state_by_name_with_default(key("tries"), Kind::Int, Value::int(3))
        .state_by_name(key("notes"), Kind::list(Kind::Str))
        .config_by_name(key("width"), Kind::Int)
        .build()
}

/// Nodes registered `zeta`, `alpha`, `mid`: registration order, not
/// alphabetical.
fn graph() -> Graph {
    let route = FnEdge::new(|_s: &State, _c: &Config| Ok(vec![to("mid")]))
        .targets([to("mid"), end("skipped")]);
    GraphBuilder::new(schema())
        .entry(nid("zeta"))
        .node(nid("zeta"), panicking())
        .join(nid("alpha"), panicking())
        .node(nid("mid"), panicking())
        .edge(nid("zeta"), route)
        .edge(nid("alpha"), Always(end("done")))
        .edge(
            nid("mid"),
            FnEdge::new(|_s: &State, _c: &Config| Ok(vec![to("alpha")])),
        )
        .input(key("topic"))
        .output(key("notes"))
        .build()
        .unwrap()
}

#[test]
fn given_a_graph_when_described_then_it_holds_its_entry_signature_schema_and_nodes_in_order() {
    assert_written(
        &json(&graph().describe()),
        &json!({
            "description_version": "br-llm-graph-description/1",
            "entry": "zeta",
            "signature": {
                "inputs": {"topic": {"type": "str"}},
                "outputs": {"notes": {"type": "list", "element": {"type": "str"}}}
            },
            "schema": {
                "state": {
                    "notes": {"type": "list", "element": {"type": "str"}},
                    "topic": {"type": "str"},
                    "tries": {"type": "int"}
                },
                "config": {"width": {"type": "int"}},
                "defaults": {"tries": {"type": "int"}}
            },
            "nodes": [
                {"id": "zeta", "kind": "node",
                 "edge": {"targets": [{"node": "mid"}, {"end": "skipped"}]}},
                {"id": "alpha", "kind": "join", "edge": {"targets": [{"end": "done"}]}},
                {"id": "mid", "kind": "node", "edge": {"undeclared": true}}
            ]
        }),
    );
}

#[test]
fn given_the_version_when_read_then_it_is_br_llm_graph_description_1() {
    assert_eq!(DESCRIPTION_VERSION, "br-llm-graph-description/1");
}

#[test]
fn given_nodes_and_edges_that_panic_when_described_then_nothing_runs() {
    let edge = PanickingEdge(vec![end("done")]);
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), panicking())
        .edge(nid("a"), edge)
        .build()
        .unwrap();
    let described = graph.describe();
    assert_eq!(described.nodes.len(), 1);
}

#[test]
fn given_two_builds_from_the_same_parts_when_described_then_equal_in_the_same_order() {
    let (first, second) = (graph().describe(), graph().describe());
    assert_eq!(first, second);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
}

#[test]
fn given_a_graph_when_described_twice_then_equal() {
    let graph = graph();
    assert_eq!(graph.describe(), graph.describe());
}

/// A host's node with its own kind label and the description of a graph it
/// runs.
struct Router {
    inner: Graph,
}

impl Node for Router {
    fn run<'a>(
        &'a self,
        _state: &'a State,
        _config: &'a Config,
        _ctx: &'a crate::graph::Context,
    ) -> crate::graph::NodeFuture<'a> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn describe(&self) -> NodeDescription {
        NodeDescription::labelled("router")
            .unwrap()
            .nesting(self.inner.describe())
    }
}

fn hosting(join: bool) -> Graph {
    let router = Router { inner: graph() };
    let builder = GraphBuilder::new(schema()).entry(nid("r"));
    let builder = if join {
        builder.join(nid("r"), router)
    } else {
        builder.node(nid("r"), router)
    };
    builder.edge(nid("r"), Always(end("done"))).build().unwrap()
}

#[test]
fn given_a_host_node_with_a_label_and_a_nested_description_when_described_then_both_are_written() {
    let node = node_json(&hosting(false).describe(), "r");
    assert_eq!(node["kind"], "router");
    let mut nested = json(&graph().describe());
    nested
        .as_object_mut()
        .unwrap()
        .shift_remove("description_version");
    assert_eq!(node["graph"], nested);
    assert!(node["graph"].get("description_version").is_none());
    assert!(node.get("join").is_none());
}

#[test]
fn given_a_labelled_host_node_registered_as_a_join_when_described_then_join_is_marked() {
    let node = node_json(&hosting(true).describe(), "r");
    assert_eq!(
        (node["kind"].clone(), node["join"].clone()),
        (json!("router"), json!(true))
    );
}

#[test]
fn given_a_host_node_nesting_without_a_label_when_described_then_node_with_its_graph() {
    struct Wrapper(Graph);
    impl Node for Wrapper {
        fn run<'a>(
            &'a self,
            _s: &'a State,
            _c: &'a Config,
            _x: &'a crate::graph::Context,
        ) -> crate::graph::NodeFuture<'a> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn describe(&self) -> NodeDescription {
            NodeDescription::default().nesting(self.0.describe())
        }
    }
    let described = GraphBuilder::new(schema())
        .entry(nid("w"))
        .join(nid("w"), Wrapper(graph()))
        .edge(nid("w"), Always(end("done")))
        .build()
        .unwrap()
        .describe();
    let node = node_json(&described, "w");
    assert_eq!(node["kind"], "join");
    assert!(node.get("join").is_none());
    assert_eq!(node["graph"]["entry"], "zeta");
}

#[test]
fn given_a_label_that_is_no_identifier_or_a_library_kind_when_labelled_then_refused() {
    for label in [
        "",
        "Router",
        "has space",
        "node",
        "join",
        "call",
        "map",
        "llm_call",
    ] {
        assert!(
            matches!(
                NodeDescription::labelled(label),
                Err(GraphError::Identifier { field: "kind_label", ref value }) if value == label
            ),
            "{label:?} should be refused"
        );
    }
    for label in ["react_agent", "generator_critic", "tools", "model_call"] {
        assert!(NodeDescription::labelled(label).is_err(), "{label}");
    }
    assert!(NodeDescription::labelled("router_2").is_ok());
}

#[test]
fn given_no_realizes_label_when_described_then_the_field_is_omitted() {
    let node = node_json(&graph().describe(), "zeta");
    assert!(node.get("realizes").is_none());
}

#[test]
fn given_realizes_labels_when_described_then_written_in_the_order_given_kept_as_given() {
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(
            nid("a"),
            noop().realizing(["Plan topics", "Plan topics", "β"]),
        )
        .join(nid("j"), noop().realizing(["Join"]).realizing(["Twice"]))
        .edge(nid("a"), Always(to("j")))
        .edge(nid("j"), Always(end("done")))
        .build()
        .unwrap();
    let described = graph.describe();
    assert_eq!(
        node_json(&described, "a")["realizes"],
        json!(["Plan topics", "Plan topics", "β"])
    );
    let join = node_json(&described, "j");
    assert_eq!(
        (join["kind"].clone(), join["realizes"].clone()),
        (json!("join"), json!(["Join", "Twice"]))
    );
}

#[test]
fn given_a_host_description_with_labels_when_wrapped_in_realizing_then_its_labels_come_first() {
    struct Labelled;
    impl Node for Labelled {
        fn run<'a>(
            &'a self,
            _s: &'a State,
            _c: &'a Config,
            _x: &'a crate::graph::Context,
        ) -> crate::graph::NodeFuture<'a> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn describe(&self) -> NodeDescription {
            NodeDescription::labelled("gate")
                .unwrap()
                .realizing(["Own"])
        }
    }
    let node = Labelled.realizing(["Added"]);
    assert_eq!(node.labels(), ["Added"]);
    let described = node.describe();
    assert_eq!(
        described.realizes,
        vec!["Own".to_owned(), "Added".to_owned()]
    );
}

#[test]
fn given_an_edge_declaring_no_target_when_described_then_its_targets_are_an_empty_list() {
    let none = FnEdge::new(|_s: &State, _c: &Config| Ok(vec![end("done")])).targets([]);
    let graph = GraphBuilder::new(schema())
        .entry(nid("a"))
        .node(nid("a"), noop())
        .edge(nid("a"), none)
        .build()
        .unwrap();
    assert_eq!(
        node_json(&graph.describe(), "a")["edge"],
        json!({"targets": []})
    );
}
