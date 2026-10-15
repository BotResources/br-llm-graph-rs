//! A ReAct agent run: its answer, its tool rounds, its round limit, a
//! resumed history and its enabled key.

use std::sync::Arc;

use br_llm_clients_core::{OutputMode, Parameter, ParameterKind, Parameters, StructuredShape};
use br_llm_clients_mock::MockClient;
use br_llm_messages::{
    InputId, Text, ToolCallId, ToolName, ToolResult, ToolResultBlock, Turn, TurnId, TurnItem,
    UserBlock, UserInput, UserSource,
};
use serde_json::json;

use crate::error::GraphError;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::round_limit::OnLimit;
use crate::react::test_support::*;
use crate::state::{Kind, Value};

#[tokio::test]
async fn given_a_plain_answer_when_the_agent_runs_then_history_and_answer_come_back() {
    let client = mock(vec![text_step("hello there")]);
    let graph = agent(&client).graph().unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "hello there");
    assert_eq!(
        state.conversation(&key("history")).unwrap().entries().len(),
        2
    );
    let requests = requests(&client);
    assert_eq!(requests.len(), 1);
    assert_eq!(
        system_of(&requests[0]).as_deref(),
        Some("You are a helpful agent.")
    );
    assert_eq!(requests[0].tools().len(), 1);
}

#[tokio::test]
async fn given_one_tool_round_when_the_agent_runs_then_the_turn_holds_the_round_and_the_reply() {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("echo said echoed"),
    ]);
    let graph = agent(&client).graph().unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "echo said echoed");
    let turn = last_turn(&state);
    assert_eq!(turn.items().len(), 3);
    assert!(matches!(turn.items()[1], TurnItem::ToolResults(_)));
}

#[tokio::test]
async fn given_a_final_step_of_several_text_blocks_when_the_agent_runs_then_answer_joins_them_verbatim()
 {
    let several = br_llm_messages::Step::new(
        vec![
            br_llm_messages::AssistantBlock::Text {
                text: Text::new("one ").unwrap(),
            },
            br_llm_messages::AssistantBlock::Text {
                text: Text::new("two").unwrap(),
            },
        ],
        br_llm_messages::StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    let client = mock(vec![tool_call_step("c1", "echo", json!({})), several]);
    let graph = agent(&client).graph().unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "one two");
}

#[tokio::test]
async fn given_a_structured_output_when_the_agent_runs_then_answer_is_the_json_text_of_the_block() {
    let shape = StructuredShape::new(
        "count",
        None,
        Parameters::new(vec![
            Parameter::new("n", "", true, ParameterKind::Integer).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        structured_step(json!({ "n": 3 })),
    ]);
    let graph = ReactAgent {
        output_mode: OutputMode::Structured(shape.clone()),
        ..agent(&client)
    }
    .graph()
    .unwrap();
    let state = finished(call(&graph, question("count")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), r#"{"n":3}"#);
    let requests = requests(&client);
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request| *request.output() == OutputMode::Structured(shape.clone()))
    );
}

#[tokio::test]
async fn given_the_round_limit_ends_the_turn_when_the_agent_runs_then_limit_reached_is_true() {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let graph = limited(
        &client,
        1,
        OnLimit::End {
            node: nid("limit"),
            flag: key("limit_reached"),
        },
    )
    .graph()
    .unwrap();
    let state = finished(call(&graph, question("hi")).await);
    assert!(state.bool(&key("limit_reached")).unwrap());
    assert!(
        state
            .str(&key("answer"))
            .unwrap()
            .starts_with("Tool round limit reached (1)")
    );
    assert_eq!(requests(&client).len(), 2);
}

#[tokio::test]
async fn given_the_round_limit_errors_when_the_agent_keeps_calling_then_the_run_fails() {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let graph = limited(&client, 1, OnLimit::Error).graph().unwrap();
    let error = call(&graph, question("hi")).await.err().unwrap().error;
    assert!(matches!(
        error,
        GraphError::ToolLimitReached { max_rounds: 1, .. }
    ));
}

#[tokio::test]
async fn given_a_history_with_a_finished_tool_round_when_called_again_then_the_turn_resumes() {
    let mut history = question("hi");
    let mut turn = Turn::new(
        TurnId::new("t0").unwrap(),
        Some(author()),
        tool_call_step("c1", "echo", json!({})),
    );
    turn.push_result(ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    ))
    .unwrap();
    history.push_turn(turn).unwrap();

    let client = mock(vec![text_step("resumed reply")]);
    let graph = agent(&client).graph().unwrap();
    let state = finished(call(&graph, history).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "resumed reply");
    assert_eq!(
        state.conversation(&key("history")).unwrap().entries().len(),
        2
    );
    let turn = last_turn(&state);
    assert_eq!(turn.id().as_str(), "t0");
    assert_eq!(turn.items().len(), 3);
    assert_eq!(requests(&client).len(), 1);
}

#[tokio::test]
async fn given_a_new_activation_when_the_round_limit_counts_then_it_starts_from_the_open_turn() {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("first"),
        tool_call_step("c2", "echo", json!({})),
        text_step("second"),
    ]);
    let graph = limited(&client, 1, OnLimit::Error).graph().unwrap();
    let ctx = ctx();
    let state = finished(call_in(&graph, question("hi"), &ctx).await);
    let mut history = state.conversation(&key("history")).unwrap().clone();
    history
        .push_input(
            UserInput::new(
                InputId::new("i-again").unwrap(),
                UserSource::Human,
                None,
                vec![UserBlock::text(Text::new("again").unwrap())],
            )
            .unwrap(),
        )
        .unwrap();
    let state = finished(call_in(&graph, history, &ctx).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "second");
}

fn selective(client: &MockClient, writer: Arc<WriteFileTool>) -> ReactAgent {
    ReactAgent {
        tools: vec![Arc::new(EchoTool), writer],
        enabled: Some(key("offered")),
        ..agent(client)
    }
}

#[tokio::test]
async fn given_an_enabled_subset_when_the_agent_runs_then_only_it_is_offered_and_another_call_is_refused()
 {
    let client = mock(vec![
        calls_step(&["echo", "write_file"]),
        text_step("could not write"),
    ]);
    let writer = Arc::new(WriteFileTool::default());
    let graph = selective(&client, writer.clone()).graph().unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("offered")),
        Some(&Kind::list(Kind::Str))
    );
    let state = graph
        .start_state([
            (key("history"), Value::conversation(question("hi"))),
            (key("offered"), Value::list(vec![Value::str("echo")])),
        ])
        .unwrap();
    let state = finished(run_agent(&graph, state).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "could not write");
    assert_eq!(
        results_of(&last_turn(&state)),
        [
            ("echo".to_owned(), false, "echoed".to_owned()),
            (
                "write_file".to_owned(),
                true,
                "Tool write_file is not enabled for this run: this call was not executed."
                    .to_owned()
            ),
        ]
    );
    assert_eq!(writer.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    let requests = requests(&client);
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_eq!(tool_names(&request), ["echo"]);
    }
}
