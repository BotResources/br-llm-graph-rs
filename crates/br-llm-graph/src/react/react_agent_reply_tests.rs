//! A ReAct agent in memory and the replies it cannot use: its final reply
//! checked against its shape, a reply that calls tools and the closing step
//! of the round limit `End` not checked, and how the run fails when no
//! retry is left.

use std::num::NonZeroUsize;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::Reply;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Count, Limit};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::failed_node;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;

#[tokio::test]
async fn given_no_reply_retries_when_the_final_reply_does_not_fit_then_the_run_fails_with_structured_in_the_model_call()
 {
    let client = mock(vec![structured_step(json!({ "answer": 42 }))]);
    let graph = asking_found(&client, None).graph().unwrap();

    let failure = call(&graph, question("where")).await.err().unwrap();

    let (node, error) = failed_node(&failure.error);
    assert_eq!(node, "llm");
    assert!(
        matches!(error, GraphError::Structured { message } if message == "`answer` is not a string"),
        "{error}"
    );
}

#[tokio::test]
async fn given_no_reply_retries_when_the_client_refuses_the_reply_then_the_run_fails_with_model_as_before()
 {
    let refused = ClientError::MalformedResponse {
        detail: "a cut JSON".to_owned(),
    };
    let client = mock(Vec::new());
    client.push_reply(Reply::failure(refused.clone()).build().unwrap());
    let graph = agent(&client).graph().unwrap();

    let failure = call(&graph, question("where")).await.err().unwrap();

    let (node, error) = failed_node(&failure.error);
    assert_eq!(node, "llm");
    assert!(matches!(error, GraphError::Model(found) if *found == refused));
}

#[tokio::test]
async fn given_a_structured_agent_when_a_reply_calls_tools_then_it_is_not_checked_against_the_shape()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        structured_step(json!({ "answer": "echoed" })),
    ]);
    let graph = asking_found(&client, None).graph().unwrap();

    let done = finished(call(&graph, question("echo")).await);

    assert_eq!(done.str(&key("answer")).unwrap(), r#"{"answer":"echoed"}"#);
}

#[tokio::test]
async fn given_a_structured_agent_when_the_round_limit_end_closes_the_turn_then_its_closing_step_is_not_checked()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let graph = ReactAgent {
        output_mode: found_shape(),
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(1).unwrap()),
            on_limit: OnLimit::End {
                node: nid("limit"),
                flag: key("limited"),
            },
        }),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    let done = finished(call(&graph, question("echo")).await);

    assert_eq!(
        done.str(&key("answer")).unwrap(),
        "Tool round limit reached (1): no final reply."
    );
    assert!(done.bool(&key("limited")).unwrap());
    assert_eq!(client.calls(), 2);
}

#[tokio::test]
async fn given_the_last_retry_when_its_reply_does_not_fit_either_then_the_run_fails_with_its_reason()
 {
    let client = mock(vec![
        structured_step(json!({ "answer": 1 })),
        structured_step(json!({ "answer": "x", "more": 1 })),
    ]);
    let graph = asking_found(&client, Some(Count::Fixed(1)))
        .graph()
        .unwrap();

    let failure = call(&graph, question("where")).await.err().unwrap();

    let (_, error) = failed_node(&failure.error);
    assert!(matches!(
        error,
        GraphError::Structured { message } if message == "`more` is not in the shape"
    ));
    assert_eq!(client.calls(), 2);
}
