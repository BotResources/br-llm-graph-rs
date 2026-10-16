//! A map holds its list key, its item key, its concurrency limit, its
//! item-failure policy and the description of its body.

use std::num::NonZeroUsize;
use std::sync::Arc;

use serde_json::json;

use super::test_support::*;
use crate::graph::{
    Always, CaptureSource, CaptureUpdate, GraphBuilder, Input, ItemFailure, Limit, Map, Node,
    Output, SubGraph,
};
use crate::state::{Kind, Schema, StateKey};

fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("topics"), Kind::list(Kind::Str))
        .state_by_name(key("topic"), Kind::Str)
        .state_by_name(key("notes"), Kind::list(Kind::Str))
        .config_by_name(key("width"), Kind::Int)
        .build()
}

fn described(map: Map) -> serde_json::Value {
    let graph = GraphBuilder::new(schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(nid("each"), Always(end("done")))
        .build()
        .unwrap();
    node_json(&graph.describe(), "each")
}

fn typed_map(body: impl Node + 'static) -> Map {
    let topics: StateKey<Vec<String>> = StateKey::declared(key("topics"));
    let topic: StateKey<String> = StateKey::declared(key("topic"));
    Map::new(&topics, &topic, body)
}

#[test]
fn given_a_map_without_limit_when_described_then_it_holds_its_keys_policy_and_body() {
    assert_written(
        &described(typed_map(noop().realizing(["Summarize"]))),
        &json!({
            "id": "each",
            "kind": "map",
            "edge": {"targets": [{"end": "done"}]},
            "list": "topics",
            "item": "topic",
            "on_item_failure": "finish",
            "body": {"kind": "node", "realizes": ["Summarize"]}
        }),
    );
}

#[test]
fn given_a_fixed_limit_and_fail_fast_when_described_then_both_are_written() {
    let map = typed_map(noop())
        .max_concurrency(Limit::Fixed(NonZeroUsize::new(3).unwrap()))
        .on_item_failure(ItemFailure::FailFast);
    let node = described(map);
    assert_eq!(node["max_concurrency"], json!({"fixed": 3}));
    assert_eq!(node["on_item_failure"], "fail_fast");
}

#[test]
fn given_a_limit_read_from_configuration_and_captures_when_described_then_both_are_written() {
    let map = typed_map(noop())
        .max_concurrency(Limit::Config(key("width")))
        .on_item_failure(ItemFailure::Capture(vec![
            CaptureUpdate::Append(key("notes"), CaptureSource::Reason),
            CaptureUpdate::Append(key("notes"), CaptureSource::From(key("topic"))),
        ]));
    let node = described(map);
    assert_eq!(node["max_concurrency"], json!({"config": "width"}));
    assert_eq!(
        node["on_item_failure"],
        json!({"capture": [
            {"target": {"append": "notes"}, "source": {"reason": true}},
            {"target": {"append": "notes"}, "source": {"from": "topic"}}
        ]})
    );
}

#[test]
fn given_a_call_as_the_body_when_described_then_the_body_is_the_call_without_id_or_edge() {
    let child_schema = Schema::builder()
        .state_by_name(key("topic"), Kind::Str)
        .state_by_name(key("note"), Kind::Str)
        .build();
    let child = GraphBuilder::new(child_schema)
        .entry(nid("write"))
        .node(nid("write"), panicking())
        .edge(nid("write"), Always(end("done")))
        .input(key("topic"))
        .output(key("note"))
        .build()
        .unwrap();
    let call = SubGraph::call(Arc::new(child))
        .input(key("topic"), Input::From(key("topic")))
        .output(key("note"), Output::Append(key("notes")));
    let body = &described(typed_map(call))["body"];
    assert_eq!(body["kind"], "call");
    assert!(body.get("id").is_none() && body.get("edge").is_none());
    assert_eq!(body["graph"]["entry"], "write");
    assert_eq!(
        body["outputs"],
        json!([{"child": "note", "target": {"append": "notes"}}])
    );
}
