//! `LlmCall` and the replies it cannot use: a structured reply checked
//! against its shape, a reply the client refuses, each asked for again
//! within the reply retries with the default reminder, and how the call
//! fails when no retry is left.

use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::Reply;
use br_llm_messages::StopReason;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::given::Model;
use crate::react::llm_call::LlmCall;
use crate::react::llm_call_support::*;
use crate::react::test_support::{
    LaxClient, key, mock, requests, returned, structured_step, text_step,
};

#[tokio::test]
async fn given_no_reply_retries_when_the_client_refuses_the_reply_then_the_call_fails_with_model_as_before()
 {
    let refused = ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    };
    let client = mock(Vec::new());
    client.push_reply(Reply::failure(refused.clone()).build().unwrap());

    let error = summary_of(summarize(&client, set_summary()))
        .await
        .unwrap_err();

    assert!(matches!(returned(&error), GraphError::Model(found) if *found == refused));
    assert_eq!(client.calls(), 1);
}

#[tokio::test]
async fn given_no_reply_retries_when_a_structured_reply_does_not_fit_then_the_call_fails_with_structured_and_writes_nothing()
 {
    let client = mock(vec![structured_step(json!({ "subject": "tides" }))]);

    let failure = run_in(
        &single(asking_facts(&client, None)),
        state_with("doc", &[]),
        &ctx(),
    )
    .await
    .err()
    .unwrap();

    assert!(
        matches!(returned(&failure.error), GraphError::Structured { message } if message == "`count` is missing"),
        "{}",
        failure.error
    );
    assert_eq!(failure.checkpoint.state.str(&key("summary")).unwrap(), "");
    assert_eq!(client.calls(), 1);
}

#[tokio::test]
async fn given_a_structured_mode_when_a_lax_client_gives_no_structured_block_then_the_reply_cannot_be_used()
 {
    let client = Arc::new(LaxClient::new(vec![text_step("two tides a day")]));
    let call = LlmCall {
        model: Model::client(client.clone()),
        output_mode: facts_mode(),
        ..summarize(&mock(Vec::new()), set_summary())
    };

    let error = summary_of(call).await.unwrap_err();

    assert!(matches!(
        returned(&error),
        GraphError::Structured { message } if message == "the reply carries no structured block"
    ));
}

#[tokio::test]
async fn given_reply_retries_when_a_reply_does_not_fit_then_the_same_request_follows_with_the_reminder_and_the_fitting_answer_is_written()
 {
    let client = mock(vec![
        structured_step(json!({ "subject": "tides", "count": "two" })),
        structured_step(json!({ "subject": "tides", "count": 2 })),
    ]);

    let answer = summary_of(asking_facts(&client, Some(Count::Fixed(2))))
        .await
        .unwrap();

    assert_eq!(answer, r#"{"subject":"tides","count":2}"#);
    let sent = requests(&client);
    assert_eq!(sent.len(), 2, "one retry was enough");
    assert_eq!(sent[0].messages(), [user_text("doc")]);
    assert_eq!(
        sent[1].messages(),
        [user_text("doc"), reminded_of("`count` is not an integer")]
    );
    assert_eq!(sent[1].system(), sent[0].system());
    assert_eq!(sent[1].output(), sent[0].output());
}

#[tokio::test]
async fn given_reply_retries_when_the_client_refuses_a_reply_as_unusable_or_malformed_then_it_is_asked_again()
 {
    for refused in [
        ClientError::ReplyUnusable {
            stop_reason: StopReason::MaxTokens,
        },
        ClientError::MalformedResponse {
            detail: "a cut JSON".to_owned(),
        },
    ] {
        let client = mock(Vec::new());
        client.push_reply(Reply::failure(refused.clone()).build().unwrap());
        client.push_reply(Reply::success(text_step("the moon")).build().unwrap());

        let answer = summary_of(LlmCall {
            reply_retries: Some(Count::Fixed(1)),
            ..summarize(&client, set_summary())
        })
        .await
        .unwrap();

        assert_eq!(answer, "the moon");
        assert_eq!(
            requests(&client)[1].messages(),
            [user_text("doc"), reminded_of(&refused.to_string())]
        );
    }
}

#[tokio::test]
async fn given_reply_retries_when_the_client_fails_otherwise_then_the_call_fails_at_once() {
    let client = mock(Vec::new());
    client.push_reply(Reply::failure(ClientError::Authentication).build().unwrap());

    let error = summary_of(LlmCall {
        reply_retries: Some(Count::Fixed(3)),
        ..summarize(&client, set_summary())
    })
    .await
    .unwrap_err();

    assert!(matches!(
        returned(&error),
        GraphError::Model(ClientError::Authentication)
    ));
    assert_eq!(client.calls(), 1);
}

#[tokio::test]
async fn given_the_last_retry_when_its_reply_does_not_fit_either_then_the_call_fails_with_its_reason()
 {
    let client = mock(vec![
        structured_step(json!({ "subject": "tides" })),
        structured_step(json!({ "subject": "tides", "count": 2, "extra": true })),
    ]);

    let error = summary_of(asking_facts(&client, Some(Count::Fixed(1))))
        .await
        .unwrap_err();

    assert!(matches!(
        returned(&error),
        GraphError::Structured { message } if message == "`extra` is not in the shape"
    ));
    assert_eq!(client.calls(), 2);
}
