//! Unreadable verdicts beyond the critic's structured block: a reply the
//! client refuses as unusable or malformed, and a block a verdict reader
//! says it cannot read, each asked for again within the verdict retries with
//! its reason, and failing the run with `Structured` after the last.

use std::sync::Arc;

use br_llm_clients_core::{ClientError, Parameter, ParameterKind, Parameters, StructuredShape};
use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::{StopReason, WireMessage};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::seats::CriticSeat;
use crate::react::test_support::*;
use crate::react::verdict::{Reading, Verdict, VerdictOverride, VerdictReader};

/// The message that asks the critic again after `reason`.
fn verdict_retry(reason: &str) -> WireMessage {
    user_message(&format!(
        "Your reply could not be read as a verdict: {reason}. Reply again with the verdict."
    ))
}

#[tokio::test]
async fn given_a_critic_reply_the_client_refuses_when_a_verdict_retry_is_left_then_it_is_an_unreadable_verdict_asked_again()
 {
    let refused = ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    };
    let critic_client = mock(Vec::new());
    critic_client.push_reply(Reply::failure(refused.clone()).build().unwrap());
    push(&critic_client, verdict(true, "Good."));
    let graph = GeneratorCritic {
        verdict_retries: Count::Fixed(1),
        ..duo(&mock(vec![text_step("draft")]), &critic_client)
    }
    .graph()
    .unwrap();

    let done = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(
        shape(done.transcript()),
        ["user", "writer:draft", "reviewer:Good."]
    );
    let sent = requests(&critic_client);
    let mut expected = sent[0].messages().to_vec();
    expected.push(verdict_retry(&refused.to_string()));
    assert_eq!(sent[1].messages(), expected.as_slice());
}

#[tokio::test]
async fn given_a_critic_reply_the_client_refuses_when_no_verdict_retry_is_left_then_structured_and_no_critique()
 {
    let refused = ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    };
    let critic_client = mock(Vec::new());
    critic_client.push_reply(Reply::failure(refused.clone()).build().unwrap());
    let graph = duo(&mock(vec![text_step("draft")]), &critic_client)
        .graph()
        .unwrap();

    let (conversation, error) = failed_in(call(&graph, question("explain")).await, "critique");

    assert!(
        matches!(error, GraphError::Structured { ref message } if *message == refused.to_string()),
        "{error}"
    );
    assert_eq!(shape(&conversation), ["user", "writer:draft"]);
}

/// A shape `grading` with `grade` and `notes`, read by a reader that says
/// a block without a grade cannot be read.
fn grading(client: &MockClient) -> CriticSeat {
    let parameters = Parameters::new(vec![
        Parameter::new("grade", "", true, ParameterKind::String).unwrap(),
        Parameter::new("notes", "", true, ParameterKind::String).unwrap(),
    ])
    .unwrap();
    let reader: VerdictReader = Arc::new(|block| {
        let Some(grade) = block["grade"].as_str() else {
            return Ok(Reading::Unreadable {
                reason: "the grade is missing".to_owned(),
            });
        };
        Ok(Reading::Verdict(Verdict {
            is_valid: grade == "pass",
            message: block["notes"].as_str().unwrap_or_default().to_owned(),
        }))
    });
    CriticSeat {
        verdict: Some(VerdictOverride {
            shape: StructuredShape::new("grading", None, parameters).unwrap(),
            reader: Some(reader),
        }),
        ..critic(client)
    }
}

#[tokio::test]
async fn given_a_reader_that_cannot_read_a_block_when_a_retry_is_left_then_the_critic_is_asked_again_with_its_reason()
 {
    let critic_client = mock(vec![
        structured_step(json!({ "notes": "fine" })),
        structured_step(json!({ "grade": "pass", "notes": "Fine." })),
    ]);
    let graph = GeneratorCritic {
        verdict_retries: Count::Fixed(1),
        ..GeneratorCritic::new(
            generator(&mock(vec![text_step("draft")])),
            grading(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();

    let done = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(
        shape(done.transcript()),
        ["user", "writer:draft", "reviewer:Fine."]
    );
    let sent = requests(&critic_client);
    assert_eq!(
        sent[1].messages().last(),
        Some(&verdict_retry("the grade is missing"))
    );
}

#[tokio::test]
async fn given_a_reader_that_cannot_read_a_block_when_no_retry_is_left_then_structured_with_its_reason()
 {
    let critic_client = mock(vec![structured_step(json!({ "notes": "fine" }))]);
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("draft")])),
        grading(&critic_client),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();

    let (conversation, error) = failed_in(call(&graph, question("explain")).await, "critique");

    assert!(
        matches!(error, GraphError::Structured { ref message } if message == "the grade is missing"),
        "{error}"
    );
    assert_eq!(shape(&conversation), ["user", "writer:draft"]);
}
