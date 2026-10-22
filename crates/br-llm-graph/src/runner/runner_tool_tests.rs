//! A runner call as a tool of a ReAct agent: keyed by the tool node's origin
//! and the tool call's id, the calls of one reply at once within the tool
//! concurrency limit, the answer's blocks as the result, a child's failure
//! and an error of the port as error results, and the turn goes on.

use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::{
    AssistantBlock, Step, StopReason, Text, ToolCall, ToolCallId, ToolName, ToolResultBlock,
    TurnItem,
};
use serde_json::{Value as Json, json};

use crate::graph::Limit;
use crate::origin::RunId;
use crate::react::react_agent_support::{agent, call, call_in, ctx, finished, last_turn, question};
use crate::react::test_gate::beside;
use crate::react::test_support::{
    mock_at, push, requests, results_of, sent_results, spec, text_step,
};
use crate::runner::test_support::{call_key, key, target};
use crate::runner::{AnswerBlock, RunnerTool};
use crate::testkit::{Scripted, TestRunners, text_block};

/// The runner tool `name` calling `runner` through `port`.
fn tool(name: &str, runner: &str, port: &Arc<TestRunners>) -> Arc<RunnerTool> {
    Arc::new(RunnerTool {
        spec: spec(name),
        target: target(runner),
        port: port.clone(),
    })
}

/// A step calling each `(id, tool, arguments)`, in order.
fn calling(calls: &[(&str, &str, Json)]) -> Step {
    let blocks = calls
        .iter()
        .map(|(id, name, arguments)| {
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new(*id).unwrap(),
                name: ToolName::new(*name).unwrap(),
                arguments: arguments.clone(),
            })
        })
        .collect();
    Step::new(blocks, StopReason::AwaitingToolResults, None, None).unwrap()
}

/// A client answering `first` with its tool call ids as written, then
/// `text`.
fn scripted(first: Step, text: &str) -> MockClient {
    let client = mock_at(br_llm_clients_core::ReasoningLevel::Medium);
    client.push_reply(Reply::success(first).build().unwrap());
    push(&client, text_step(text));
    client
}

/// The answer of a call of `ask_a_document`: what the document it is asked
/// about says.
fn documents() -> Arc<TestRunners> {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("d1 and d2 can answer")]);
    port.script("ask_a_document", |request| {
        let document = request
            .arguments
            .get("document_id")
            .and_then(Json::as_str)
            .unwrap_or("nothing");
        Scripted::Answer(vec![text_block(&format!("{document} says yes"))])
    });
    port
}

#[tokio::test]
async fn given_runner_tools_called_in_one_reply_when_run_then_they_run_at_once_within_the_tool_limit_each_under_its_key()
 {
    let port = documents();
    port.hold("documentalist");
    port.hold("ask_a_document");
    let client = scripted(
        calling(&[
            ("c1", "select_documents", json!({"question": "tides?"})),
            ("c2", "ask_a_document", json!({"document_id": "d1"})),
            ("c3", "ask_a_document", json!({"document_id": "d2"})),
        ]),
        "both say yes",
    );
    let mut agent = agent(&client);
    agent.tools = vec![
        tool("select_documents", "documentalist", &port),
        tool("ask_a_document", "ask_a_document", &port),
    ];
    agent.tool_concurrency = Some(Limit::Fixed(NonZeroUsize::new(2).unwrap()));
    let graph = agent.graph().unwrap();
    let check = async {
        port.waiting(2).await;
        let started = port.calls().len();
        port.release("documentalist");
        port.release("ask_a_document");
        started
    };
    let ctx = ctx().with_run_id(RunId::new("run-1"));

    let (called, started) = match beside(call_in(&graph, question("tides?"), &ctx), check).await {
        Ok(both) => both,
        Err(ended) => panic!(
            "the run ended before two calls waited: {:?}",
            ended.err().is_none()
        ),
    };

    assert_eq!(started, 2, "no third call starts while two wait");
    assert_eq!(port.peak(), 2);
    let done = finished(called);
    assert_eq!(done.str(&key("answer")).unwrap(), "both say yes");
    assert_eq!(
        results_of(&last_turn(&done)),
        [
            ("select_documents", "d1 and d2 can answer"),
            ("ask_a_document", "d1 says yes"),
            ("ask_a_document", "d2 says yes"),
        ]
        .map(|(name, text)| (name.to_owned(), false, text.to_owned()))
    );
    let mut keys = port.keys();
    keys.sort_by_key(|key| key.tool_call.clone().map(|id| id.as_str().to_owned()));
    assert_eq!(
        keys,
        ["c1", "c2", "c3"].map(|id| call_key("run-1", "tools", Some(id)))
    );
    let asked = port.calls();
    let selection = asked
        .iter()
        .find(|request| request.target.as_str() == "documentalist")
        .unwrap();
    assert_eq!(
        Json::Object(selection.arguments.clone()),
        json!({"question": "tides?"})
    );
}

#[tokio::test]
async fn given_a_structured_answer_when_the_tool_runs_then_its_result_holds_each_block_in_order_a_structured_one_as_json_text()
 {
    let port = Arc::new(TestRunners::new());
    port.answer(
        "documentalist",
        vec![
            text_block("The selection:"),
            AnswerBlock::Structured(json!({"document_ids": ["d1"]})),
        ],
    );
    let client = scripted(
        calling(&[("c1", "select_documents", json!({"question": "tides?"}))]),
        "done",
    );
    let mut agent = agent(&client);
    agent.tools = vec![tool("select_documents", "documentalist", &port)];
    let done = finished(call(&agent.graph().unwrap(), question("tides?")).await);

    let turn = last_turn(&done);
    let results: Vec<_> = turn
        .items()
        .iter()
        .filter_map(|item| match item {
            TurnItem::ToolResults(results) => Some(results.results().to_vec()),
            TurnItem::Step(_) => None,
        })
        .flatten()
        .collect();
    let [result] = results.as_slice() else {
        panic!("one result, got {results:?}");
    };
    assert!(!result.is_error);
    assert_eq!(
        result.content,
        [
            ToolResultBlock::text(Text::new("The selection:").unwrap()),
            ToolResultBlock::text(Text::new(r#"{"document_ids":["d1"]}"#).unwrap()),
        ]
    );
}

/// The result the model reads back for one call of `select_documents` on
/// `port`, called with `arguments`, and the calls the port received.
async fn answer_read_back(
    port: &Arc<TestRunners>,
    arguments: Json,
) -> ((String, bool, String), usize) {
    let client = scripted(
        calling(&[("c1", "select_documents", arguments)]),
        "going on",
    );
    let mut agent = agent(&client);
    agent.tools = vec![tool("select_documents", "documentalist", port)];
    let done = finished(call(&agent.graph().unwrap(), question("tides?")).await);
    assert_eq!(
        done.str(&key("answer")).unwrap(),
        "going on",
        "the turn goes on"
    );
    let sent = sent_results(&requests(&client)[1]);
    let [result] = sent.as_slice() else {
        panic!("one result sent, got {sent:?}");
    };
    (result.clone(), port.calls().len())
}

#[tokio::test]
async fn given_a_child_that_fails_when_the_tool_runs_then_an_error_result_holds_the_reason_and_the_turn_goes_on()
 {
    let port = Arc::new(TestRunners::new());
    port.fail("documentalist", "the child did not answer in time");
    assert_eq!(
        answer_read_back(&port, json!({})).await,
        (
            (
                "select_documents".to_owned(),
                true,
                "the child did not answer in time".to_owned()
            ),
            1
        )
    );
    let blank = Arc::new(TestRunners::new());
    blank.fail("documentalist", " ");
    assert_eq!(
        answer_read_back(&blank, json!({})).await.0,
        (
            "select_documents".to_owned(),
            true,
            "runner call failed".to_owned()
        )
    );
}

#[tokio::test]
async fn given_a_port_that_fails_when_the_tool_runs_then_an_error_result_holds_its_message() {
    let port = Arc::new(TestRunners::new());
    port.break_down("documentalist", "the bus is down");
    assert_eq!(
        answer_read_back(&port, json!({})).await,
        (
            (
                "select_documents".to_owned(),
                true,
                "the bus is down".to_owned()
            ),
            1
        )
    );
}

#[tokio::test]
async fn given_arguments_that_are_not_an_object_when_the_tool_runs_then_an_error_result_and_the_port_is_not_called()
 {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("unused")]);
    assert_eq!(
        answer_read_back(&port, json!(["tides?"])).await,
        (
            (
                "select_documents".to_owned(),
                true,
                "the arguments of a runner call must be a JSON object".to_owned()
            ),
            0
        )
    );
}
