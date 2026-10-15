//! What a tool error gives: an error result holding the error's message, or
//! `tool call failed` when the message is blank.

use std::sync::Arc;

use br_llm_clients_core::ToolSpec;
use serde_json::{Value as Json, json};

use crate::react::react_hook_support::{answered, call, pending, run_tools, tool_graph};
use crate::react::test_support::spec;
use crate::react::tool::{Tool, ToolFuture};
use crate::state::{Config, State};

/// An error whose message is the text it holds, blank or not.
#[derive(Debug)]
struct Broken(&'static str);

impl std::fmt::Display for Broken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Broken {}

/// A tool whose every call fails with `Broken(message)`.
struct FailingTool {
    spec: ToolSpec,
    message: &'static str,
}

impl Tool for FailingTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn call<'a>(
        &'a self,
        _arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move { Err(Broken(self.message).into()) })
    }
}

async fn answer_of(message: &'static str) -> Vec<(String, bool, String)> {
    let tool = Arc::new(FailingTool {
        spec: spec("fetch"),
        message,
    });
    answered(
        run_tools(
            &tool_graph(vec![tool], None, false),
            pending(vec![call("c1", "fetch", json!({}))]),
        )
        .await,
    )
}

#[tokio::test]
async fn given_a_tool_error_with_a_message_when_the_call_runs_then_an_error_result_holds_it() {
    assert_eq!(
        answer_of("the network is down").await,
        [("fetch".to_owned(), true, "the network is down".to_owned())]
    );
}

#[tokio::test]
async fn given_a_tool_error_with_a_blank_message_when_the_call_runs_then_tool_call_failed() {
    for blank in ["", " \n"] {
        assert_eq!(
            answer_of(blank).await,
            [("fetch".to_owned(), true, "tool call failed".to_owned())],
            "{blank:?}"
        );
    }
}
