//! A verdict shape of the caller's: sent instead of the default one, read by
//! the caller's reader, or checked at build to read as the default verdict.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{Parameter, ParameterKind, Parameters, ReasoningLevel, StructuredShape};
use br_llm_clients_mock::MockClient;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::generator_critic_verdict_tests::{critic_at, judged_with, shape_sent};
use crate::react::given::Prompt;
use crate::react::seats::CriticSeat;
use crate::react::test_support::*;
use crate::react::verdict::{Verdict, VerdictOverride, VerdictReader};

/// A shape of the caller's: a grade among `pass` and `fail`, and notes.
fn graded_shape() -> StructuredShape {
    StructuredShape::new(
        "grading",
        Some("How the answer is graded".to_owned()),
        Parameters::new(vec![
            Parameter::new(
                "grade",
                "",
                true,
                ParameterKind::Enum {
                    values: vec!["pass".to_owned(), "fail".to_owned()],
                },
            )
            .unwrap(),
            Parameter::new("notes", "", true, ParameterKind::String).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap()
}

fn graded_reader() -> VerdictReader {
    Arc::new(|value| {
        let grade = value["grade"]
            .as_str()
            .ok_or_else(|| GraphError::Structured {
                message: "no grade".to_owned(),
            })?;
        Ok(Verdict {
            is_valid: grade == "pass",
            message: value["notes"].as_str().unwrap_or_default().to_owned(),
        })
    })
}

fn graded(client: &MockClient) -> CriticSeat {
    CriticSeat {
        thinking_description: Some(Prompt::from_state(&skey("ignored"))),
        verdict: Some(VerdictOverride {
            shape: graded_shape(),
            reader: Some(graded_reader()),
        }),
        ..critic(client)
    }
}

#[tokio::test]
async fn given_an_override_with_a_reader_when_the_critic_rejects_then_its_shape_is_sent_and_the_reader_gives_the_verdict()
 {
    let generator_client = mock(vec![text_step("draft"), text_step("better")]);
    let critic_client = critic_at(
        ReasoningLevel::Off,
        vec![
            structured_step(json!({ "grade": "fail", "notes": "too short" })),
            structured_step(json!({ "grade": "pass", "notes": "" })),
        ],
    );
    let graph = GeneratorCritic::new(
        generator(&generator_client),
        graded(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();
    assert!(
        !graph.signature().inputs.contains_key(&key("ignored")),
        "the descriptions are ignored with an override"
    );
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(
        shape_of(&state, "conversation"),
        [
            "user",
            "writer:draft",
            "reviewer:too short",
            "writer:better"
        ]
    );
    let requests = requests(&critic_client);
    assert_eq!(requests.len(), 2);
    for request in requests {
        assert_eq!(shape_sent(&request), &graded_shape());
    }
}

#[tokio::test]
async fn given_a_reader_error_when_the_critic_judges_then_the_run_fails_with_it_as_returned() {
    let critic_client = critic_at(
        ReasoningLevel::Off,
        vec![structured_step(json!({ "notes": "no grade given" }))],
    );
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("draft")])),
        graded(&critic_client),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let (state, error) = failed_in(call(&graph, question("explain")).await, "critique");
    assert!(
        matches!(error, GraphError::Structured { ref message } if message == "no grade"),
        "{error}"
    );
    assert_eq!(shape_of(&state, "conversation"), ["user", "writer:draft"]);
}

#[tokio::test]
async fn given_a_reader_that_rejects_with_a_blank_message_when_the_critic_judges_then_structured() {
    let critic_client = critic_at(
        ReasoningLevel::Off,
        vec![structured_step(json!({ "grade": "fail", "notes": " \n" }))],
    );
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("draft")])),
        graded(&critic_client),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let (_, error) = failed_in(call(&graph, question("explain")).await, "critique");
    assert!(
        matches!(error, GraphError::Structured { ref message } if message.contains("blank message")),
        "{error}"
    );
}

/// A shape readable as the default verdict, with a parameter of its own.
fn scored_shape(parameters: Vec<Parameter>) -> StructuredShape {
    StructuredShape::new("scored", None, Parameters::new(parameters).unwrap()).unwrap()
}

fn parameter(name: &str, kind: ParameterKind, required: bool) -> Parameter {
    Parameter::new(name, "", required, kind).unwrap()
}

#[tokio::test]
async fn given_an_override_without_a_reader_when_the_critic_judges_then_the_shape_is_sent_as_given()
{
    let shape = scored_shape(vec![
        parameter("score", ParameterKind::Integer, true),
        parameter("is_valid", ParameterKind::Boolean, true),
        parameter("message", ParameterKind::String, true),
    ]);
    let client = critic_at(
        ReasoningLevel::Off,
        vec![structured_step(
            json!({ "score": 9, "is_valid": true, "message": "" }),
        )],
    );
    let seat = CriticSeat {
        verdict: Some(VerdictOverride {
            shape: shape.clone(),
            reader: None,
        }),
        message_description: Some(Prompt::from_state(&skey("ignored"))),
        ..critic(&client)
    };
    let (request, _) = judged_with(seat, &client, Vec::new(), BTreeMap::new()).await;
    assert_eq!(shape_sent(&request), &shape);
}

#[test]
fn given_an_override_shape_that_cannot_be_read_without_a_reader_when_built_then_verdict_shape_mismatch()
 {
    let is_valid = || parameter("is_valid", ParameterKind::Boolean, true);
    let message = || parameter("message", ParameterKind::String, true);
    let built = |shape: StructuredShape, reader: Option<VerdictReader>| {
        GeneratorCritic::new(
            generator(&mock(Vec::new())),
            CriticSeat {
                verdict: Some(VerdictOverride { shape, reader }),
                ..critic(&mock(Vec::new()))
            },
            Count::Fixed(1),
        )
        .graph()
    };
    for (parameters, reason) in [
        (vec![message()], "is_valid is not a boolean parameter"),
        (
            vec![
                parameter("is_valid", ParameterKind::String, true),
                message(),
            ],
            "is_valid is not a boolean parameter",
        ),
        (vec![is_valid()], "message is not a string parameter"),
        (
            vec![
                is_valid(),
                parameter(
                    "message",
                    ParameterKind::Array {
                        items: Box::new(ParameterKind::String),
                    },
                    true,
                ),
            ],
            "message is not a string parameter",
        ),
        (
            vec![
                is_valid(),
                parameter("message", ParameterKind::String, false),
            ],
            "is_valid and message are not both required",
        ),
        (
            vec![
                parameter("is_valid", ParameterKind::Boolean, false),
                message(),
            ],
            "is_valid and message are not both required",
        ),
    ] {
        let refused = built(scored_shape(parameters), None);
        assert!(
            matches!(refused, Err(GraphError::VerdictShapeMismatch { reason: found }) if found == reason),
            "{reason}"
        );
    }
    // Positive controls: the same refused shape is the caller's concern with
    // a reader, and a readable shape is accepted without one.
    assert!(built(scored_shape(vec![message()]), Some(graded_reader())).is_ok());
    assert!(built(scored_shape(vec![message(), is_valid()]), None).is_ok());
}
