//! An LLM call's description: how its model and system are given, its
//! request and its output.

use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use serde_json::json;

use crate::describe::test_support::{assert_written, node_json};
use crate::graph::{Always, GraphBuilder, Target};
use crate::react::given::{History, Model, Prompt, Reads};
use crate::react::llm_call::{LlmCall, LlmOutput, LlmRequest};
use crate::react::test_support::{author, mock, skey};
use crate::state::{Kind, Schema};
use crate::value::{EndLabel, NodeId};

fn described(call: LlmCall) -> serde_json::Value {
    let schema = Schema::builder()
        .state_by_name(crate::react::test_support::key("topic"), Kind::Str)
        .state_by_name(crate::react::test_support::key("chat"), Kind::Conversation)
        .state_by_name(crate::react::test_support::key("note"), Kind::Str)
        .state_by_name(
            crate::react::test_support::key("notes"),
            Kind::list(Kind::Str),
        )
        .build();
    let id = NodeId::new("summarize").unwrap();
    let graph = GraphBuilder::new(schema)
        .entry(id.clone())
        .node(id.clone(), call)
        .edge(id, Always(Target::End(EndLabel::new("done").unwrap())))
        .build()
        .unwrap();
    node_json(&graph.describe(), "summarize")
}

fn call(request: LlmRequest, output: LlmOutput) -> LlmCall {
    LlmCall {
        model: Model::client(Arc::new(mock(Vec::new()))),
        system: Prompt::from_state(&skey("topic")),
        request,
        output_mode: OutputMode::Plain,
        output,
    }
}

#[test]
fn given_a_text_request_when_described_then_its_subjects_request_and_output_are_written() {
    let request = LlmRequest::Text(Prompt::from_fn(Reads::new(), |_| {
        Ok("Summarize.".to_owned())
    }));
    assert_written(
        &described(call(request, LlmOutput::Set(skey("note")))),
        &json!({
            "id": "summarize", "kind": "llm_call",
            "edge": {"targets": [{"end": "done"}]},
            "model": "fixed", "system": "ready_made",
            "request": {"text": "function"},
            "output": {"set": "note"}
        }),
    );
}

#[test]
fn given_a_conversation_request_when_described_then_its_key_and_author_are_written() {
    let request = LlmRequest::Conversation {
        key: skey("chat"),
        author: author(),
    };
    let node = described(call(request, LlmOutput::Append(skey("notes"))));
    assert_written(
        &node["request"],
        &json!({"conversation": "chat", "author": {"id": "agent"}}),
    );
    assert_eq!(node["output"], json!({"append": "notes"}));
}

#[test]
fn given_a_history_request_when_described_then_how_it_is_given() {
    let request = LlmRequest::History(History::value(Vec::new()));
    assert_eq!(
        described(call(request, LlmOutput::Set(skey("note"))))["request"],
        json!({"history": "fixed"})
    );
}

#[test]
fn given_functions_that_panic_when_described_then_none_is_called() {
    let call = LlmCall {
        model: Model::from_fn(Reads::new(), |_| panic!("a description asks no model")),
        system: Prompt::from_fn(Reads::new(), |_| panic!("a description builds no system")),
        request: LlmRequest::History(History::from_fn(Reads::new(), |_| {
            panic!("a description builds no request")
        })),
        output_mode: OutputMode::Plain,
        output: LlmOutput::Set(skey("note")),
    };
    let node = described(call);
    assert_eq!(
        (
            node["model"].clone(),
            node["system"].clone(),
            node["request"].clone()
        ),
        (
            json!("function"),
            json!("function"),
            json!({"history": "function"})
        )
    );
}
