//! A call holds the description of the graph it runs and its mappings:
//! each input, configuration key, output and capture, with its source or
//! target, and its failure policy. A fixed value is written as its kind.

use std::sync::Arc;

use serde_json::json;

use super::test_support::*;
use crate::graph::{
    Always, CaptureSource, CaptureUpdate, Graph, GraphBuilder, Input, OnFailure, Output, SubGraph,
};
use crate::state::{Kind, Schema, Value};

fn child() -> Arc<Graph> {
    let schema = Schema::builder()
        .state_by_name(key("text"), Kind::Str)
        .state_by_name(key("tags"), Kind::list(Kind::Str))
        .state_by_name(key("answer"), Kind::Str)
        .config_by_name(key("rounds"), Kind::Int)
        .config_by_name(key("tone"), Kind::Str)
        .build();
    Arc::new(
        GraphBuilder::new(schema)
            .entry(nid("refine"))
            .node(nid("refine"), panicking())
            .edge(nid("refine"), Always(end("approved")))
            .input(key("text"))
            .input(key("tags"))
            .output(key("answer"))
            .build()
            .unwrap(),
    )
}

/// A child without inputs and configuration keys, its output `answer`.
fn bare_child() -> Arc<Graph> {
    let schema = Schema::builder()
        .state_by_name(key("answer"), Kind::Str)
        .build();
    Arc::new(
        GraphBuilder::new(schema)
            .entry(nid("refine"))
            .node(nid("refine"), panicking())
            .edge(nid("refine"), Always(end("approved")))
            .output(key("answer"))
            .build()
            .unwrap(),
    )
}

fn parent_schema() -> Schema {
    Schema::builder()
        .state_by_name(key("question"), Kind::Str)
        .state_by_name(key("result"), Kind::Str)
        .state_by_name(key("results"), Kind::list(Kind::Str))
        .state_by_name(key("labels"), Kind::list(Kind::Str))
        .state_by_name(key("error"), Kind::Str)
        .state_by_name(key("errors"), Kind::list(Kind::Str))
        .state_by_name(key("batches"), Kind::list(Kind::list(Kind::Str)))
        .state_by_name(key("matrix"), Kind::list(Kind::list(Kind::Str)))
        .state_by_name(key("grid"), Kind::list(Kind::list(Kind::Str)))
        .config_by_name(key("rounds"), Kind::Int)
        .build()
}

fn described(call: SubGraph) -> serde_json::Value {
    let graph = GraphBuilder::new(parent_schema())
        .entry(nid("ask"))
        .subgraph(nid("ask"), call)
        .edge(nid("ask"), Always(end("done")))
        .build()
        .unwrap();
    node_json(&graph.describe(), "ask")
}

#[test]
fn given_a_call_when_described_then_it_holds_its_mappings_in_order_and_the_child_without_version() {
    let call = SubGraph::call(child())
        .input(key("text"), Input::From(key("question")))
        .input(
            key("tags"),
            Input::Const(Value::list(vec![Value::str("secret tag")])),
        )
        .config(key("tone"), Input::Const(Value::str("a secret tone")))
        .config(key("rounds"), Input::Config(key("rounds")))
        .output(key("answer"), Output::Set(key("result")))
        .output(key("answer"), Output::Append(key("results")))
        .output_end_label(Output::Append(key("labels")));
    let node = described(call);
    let mut child = json(&child().describe());
    child
        .as_object_mut()
        .unwrap()
        .shift_remove("description_version");
    assert_written(
        &node,
        &json!({
            "id": "ask",
            "kind": "call",
            "edge": {"targets": [{"end": "done"}]},
            "inputs": [
                {"child": "text", "source": {"from": "question"}},
                {"child": "tags", "source": {"const": {"type": "list", "element": {"type": "str"}}}}
            ],
            "config": [
                {"child": "tone", "source": {"const": {"type": "str"}}},
                {"child": "rounds", "source": {"config": "rounds"}}
            ],
            "outputs": [
                {"child": "answer", "target": {"set": "result"}},
                {"child": "answer", "target": {"append": "results"}}
            ],
            "end_label": {"append": "labels"},
            "on_failure": "propagate",
            "graph": child
        }),
    );
}

#[test]
fn given_a_call_mapping_nothing_when_described_then_its_empty_mappings_are_omitted() {
    let node = described(SubGraph::call(bare_child()));
    for field in ["inputs", "config", "outputs", "end_label"] {
        assert!(node.get(field).is_none(), "{field}");
    }
    assert_eq!(node["on_failure"], "propagate");
}

#[test]
fn given_a_capturing_call_when_described_then_each_capture_with_its_target_and_source() {
    let call = SubGraph::call(bare_child()).on_failure(OnFailure::Capture(vec![
        CaptureUpdate::Set(key("error"), CaptureSource::Reason),
        CaptureUpdate::Append(key("errors"), CaptureSource::From(key("question"))),
        CaptureUpdate::Set(key("result"), CaptureSource::Const(Value::str("a secret"))),
        CaptureUpdate::Append(
            key("batches"),
            CaptureSource::Const(Value::list(Vec::new())),
        ),
    ]));
    assert_written(
        &described(call)["on_failure"],
        &json!({"capture": [
            {"target": {"set": "error"}, "source": {"reason": true}},
            {"target": {"append": "errors"}, "source": {"from": "question"}},
            {"target": {"set": "result"}, "source": {"const": {"type": "str"}}},
            {"target": {"append": "batches"}, "source": {"const": {"type": "list"}}}
        ]}),
    );
}

#[test]
fn given_a_call_of_a_plain_graph_when_described_then_its_kind_is_call_and_join_when_registered_so()
{
    let graph = GraphBuilder::new(parent_schema())
        .entry(nid("ask"))
        .join(nid("ask"), SubGraph::call(bare_child()))
        .edge(nid("ask"), Always(end("done")))
        .build()
        .unwrap();
    let node = node_json(&graph.describe(), "ask");
    assert_eq!(
        (node["kind"].clone(), node["join"].clone()),
        (json!("call"), json!(true))
    );
}

#[test]
fn given_empty_lists_mapped_as_fixed_values_when_described_then_each_takes_its_child_keys_kind() {
    // A state key and a configuration key of the child share the name
    // `tags`, with two list kinds: each mapping reads its own.
    let schema = Schema::builder()
        .state_by_name(key("tags"), Kind::list(Kind::Str))
        .config_by_name(key("tags"), Kind::list(Kind::Int))
        .build();
    let child = GraphBuilder::new(schema)
        .entry(nid("tag"))
        .node(nid("tag"), panicking())
        .edge(nid("tag"), Always(end("done")))
        .input(key("tags"))
        .build()
        .unwrap();
    let call = SubGraph::call(Arc::new(child))
        .input(key("tags"), Input::Const(Value::list(Vec::new())))
        .config(key("tags"), Input::Const(Value::list(Vec::new())));
    let node = described(call);
    assert_eq!(
        node["inputs"],
        json!([{"child": "tags", "source": {"const": {"type": "list", "element": {"type": "str"}}}}])
    );
    assert_eq!(
        node["config"],
        json!([{"child": "tags", "source": {"const": {"type": "list", "element": {"type": "int"}}}}])
    );
}

#[test]
fn given_a_description_of_a_call_when_nesting_a_graph_then_it_is_unchanged() {
    let call = SubGraph::call(bare_child());
    let described = crate::graph::Node::describe(&call);
    assert_eq!(described.clone().nesting(child().describe()), described);
}

#[test]
fn given_capture_lists_of_lists_when_described_then_the_element_kind_is_the_first_one_a_value_gives()
 {
    let words = |items: &[&str]| Value::list(items.iter().map(|item| Value::str(*item)).collect());
    let call = SubGraph::call(bare_child()).on_failure(OnFailure::Capture(vec![
        CaptureUpdate::Set(
            key("matrix"),
            CaptureSource::Const(Value::list(vec![words(&["a"])])),
        ),
        CaptureUpdate::Set(
            key("grid"),
            CaptureSource::Const(Value::list(vec![words(&[]), words(&["a"])])),
        ),
        CaptureUpdate::Set(
            key("batches"),
            CaptureSource::Const(Value::list(vec![words(&[]), words(&[])])),
        ),
    ]));
    let nested = json!({"type": "list", "element": {"type": "list", "element": {"type": "str"}}});
    assert_written(
        &described(call)["on_failure"],
        &json!({"capture": [
            {"target": {"set": "matrix"}, "source": {"const": nested}},
            {"target": {"set": "grid"}, "source": {"const": nested}},
            {"target": {"set": "batches"}, "source": {"const": {"type": "list"}}}
        ]}),
    );
}
