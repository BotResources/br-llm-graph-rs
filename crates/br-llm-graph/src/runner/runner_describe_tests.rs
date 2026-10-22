//! A runner call's description: its kind `runner_call`, its target, how its
//! arguments are given (fixed or a function) and never their value, and its
//! outputs; as a node of a graph and as the body of a map.

use std::sync::Arc;

use serde_json::{Map, Value as Json, json};

use crate::describe::NodeDescription;
use crate::describe::test_support::{json, node_json};
use crate::error::GraphError;
use crate::graph::{ItemFailure, TextOutput};
use crate::runner::test_support::*;
use crate::runner::{RunnerArguments, RunnerCall};
use crate::testkit::TestRunners;

#[test]
fn given_a_runner_call_with_fixed_arguments_when_described_then_its_target_and_outputs_are_written_never_its_arguments()
 {
    let port = Arc::new(TestRunners::new());
    let node = RunnerCall {
        arguments: RunnerArguments::value(Map::from_iter([(
            "passport".to_owned(),
            Json::from("sk-secret-passport"),
        )])),
        thread: Some(TextOutput::Set(skey("thread"))),
        ..ask(&port, set_answer())
    };
    let described = alone(node).describe();

    assert_eq!(
        node_json(&described, "ask"),
        json!({
            "id": "ask", "kind": "runner_call",
            "edge": {"targets": [{"end": "done"}]},
            "target": "documentalist",
            "arguments": "fixed",
            "output": {"set": "answer"},
            "thread": {"set": "thread"}
        })
    );
    let text = serde_json::to_string(&json(&described)).unwrap();
    assert!(!text.contains("sk-secret-passport"), "{text}");
    assert!(!text.contains("passport"), "{text}");
}

#[test]
fn given_a_runner_call_with_arguments_from_a_function_as_a_map_body_when_described_then_function_and_no_thread()
 {
    let port = Arc::new(TestRunners::new());
    let described = mapped(ask_about_document(&port), Some(2), ItemFailure::Finish).describe();

    assert_eq!(
        node_json(&described, "each")["body"],
        json!({
            "kind": "runner_call",
            "target": "documentalist",
            "arguments": "function",
            "output": {"append": "answers"}
        })
    );
}

#[test]
fn given_the_runner_call_kind_as_a_host_label_when_given_then_refused() {
    assert!(matches!(
        NodeDescription::labelled("runner_call"),
        Err(GraphError::Identifier { field: "kind_label", value }) if value == "runner_call"
    ));
}
