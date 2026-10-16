//! Verdict retries: an unreadable verdict asked again while retries are
//! left, with the critic's request and the retry message, the attempt number
//! one higher; nothing of a retry recorded, nothing of it counted.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use br_llm_clients_core::Client;
use br_llm_messages::WireMessage;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::generator_critic_verdict_tests::critic_at;
use crate::react::given::{Model, Reads};
use crate::react::seats::CriticSeat;
use crate::react::test_support::*;
use crate::react::verdict::{Verdict, VerdictOverride};
use crate::state::{Kind, Value};

/// The message that asks the critic again after `reason`.
fn retry(reason: &str) -> WireMessage {
    user_message(&format!(
        "Your reply could not be read as a verdict: {reason}. Reply again with the verdict."
    ))
}

/// A generator-critic whose critic sits on `critic_client`, with `retries`
/// and a budget of 2.
fn retrying(
    generator_client: &br_llm_clients_mock::MockClient,
    critic: CriticSeat,
    retries: Count,
) -> Arc<crate::graph::Graph> {
    GeneratorCritic {
        verdict_retries: retries,
        ..GeneratorCritic::new(generator(generator_client), critic, Count::Fixed(2))
    }
    .graph()
    .unwrap()
}

#[tokio::test]
async fn given_an_unreadable_verdict_and_a_retry_left_when_the_critic_judges_then_it_is_asked_again_and_only_the_verdict_is_recorded()
 {
    let critic_client = mock(vec![
        structured_step(json!({ "thinking": "hm", "is_valid": "yes", "message": "x" })),
        verdict(true, "Good."),
    ]);
    let graph = retrying(
        &mock(vec![text_step("draft")]),
        critic(&critic_client),
        Count::Fixed(1),
    );

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(
        shape(state.transcript()),
        ["user", "writer:draft", "reviewer:Good."]
    );
    let requests = requests(&critic_client);
    assert_eq!(requests.len(), 2);
    let mut expected = requests[0].messages().to_vec();
    expected.push(retry("invalid type: string \"yes\", expected a boolean"));
    assert_eq!(requests[1].messages(), expected.as_slice());
    assert_eq!(requests[1].system(), requests[0].system());
    assert_eq!(requests[1].output(), requests[0].output());
}

#[tokio::test]
async fn given_a_rejection_with_a_blank_message_when_retries_are_left_then_it_is_asked_again() {
    let critic_client = mock(vec![verdict(false, " "), verdict(false, "Cite a source.")]);
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            generator(&mock(vec![text_step("draft")])),
            critic(&critic_client),
            Count::Fixed(1),
        )
    };
    let graph = GeneratorCritic {
        verdict_retries: Count::Fixed(1),
        ..graph
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "exhausted");

    assert_eq!(
        shape(state.transcript()),
        ["user", "writer:draft", "reviewer:Cite a source."]
    );
    assert_eq!(
        requests(&critic_client)[1].messages().last(),
        Some(&retry("a rejecting verdict carries a blank message"))
    );
}

#[tokio::test]
async fn given_unreadable_verdicts_past_the_last_retry_when_the_critic_judges_then_structured_and_nothing_recorded()
 {
    let critic_client = mock(vec![
        structured_step(json!({ "thinking": "hm" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false, "message": "" })),
    ]);
    let graph = retrying(
        &mock(vec![text_step("draft")]),
        critic(&critic_client),
        Count::Fixed(1),
    );

    let (conversation, error) = failed_in(call(&graph, question("explain")).await, "critique");

    assert!(
        matches!(error, GraphError::Structured { ref message } if message == "a rejecting verdict carries a blank message"),
        "{error}"
    );
    assert_eq!(shape(&conversation), ["user", "writer:draft"]);
    assert_eq!(requests(&critic_client).len(), 2);
}

#[tokio::test]
async fn given_retries_read_from_the_configuration_when_the_critic_judges_then_they_bound_its_calls()
 {
    let critic_client = mock(vec![
        structured_step(json!({ "thinking": "hm" })),
        structured_step(json!({ "thinking": "hm" })),
        verdict(true, ""),
    ]);
    let graph = retrying(
        &mock(vec![text_step("draft")]),
        critic(&critic_client),
        Count::Config(key("retries")),
    );
    assert_eq!(graph.schema().config.get(&key("retries")), Some(&Kind::Int));

    ended(
        call_with(
            &graph,
            question("explain"),
            Vec::new(),
            BTreeMap::from([(key("retries"), Value::int(2))]),
        )
        .await,
        "validated",
    );

    assert_eq!(requests(&critic_client).len(), 3);
}

#[tokio::test]
async fn given_a_model_function_reading_the_attempt_when_a_verdict_is_retried_then_it_gives_another_client()
 {
    let first = mock(vec![structured_step(json!({ "thinking": "hm" }))]);
    let second = mock(vec![verdict(false, "Too vague.")]);
    let clients: [Arc<dyn Client>; 2] = [Arc::new(first.clone()), Arc::new(second.clone())];
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let seen = attempts.clone();
    let graph = GeneratorCritic {
        verdict_retries: Count::Fixed(1),
        ..GeneratorCritic::new(
            generator(&mock(vec![text_step("draft"), text_step("precise")])),
            CriticSeat {
                model: Model::from_fn(Reads::new(), move |ctx| {
                    seen.lock()
                        .unwrap()
                        .push((ctx.attempt(), ctx.answer().to_owned()));
                    Ok(clients[ctx.attempt().min(2) - 1].clone())
                }),
                ..critic(&first)
            },
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();

    let done = ended(call(&graph, question("explain")).await, "exhausted");

    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
    assert_eq!(
        *attempts.lock().unwrap(),
        [(1, "draft".to_owned()), (2, "draft".to_owned())]
    );
    assert_eq!(
        shape(done.transcript()),
        [
            "user",
            "writer:draft",
            "reviewer:Too vague.",
            "writer:precise",
        ],
        "a retry is not a critique: the one critique the budget allows, then the final generation"
    );
}

#[tokio::test]
async fn given_a_reader_error_when_retries_are_left_then_the_run_fails_with_it_without_a_retry() {
    let critic_client = critic_at(
        br_llm_clients_core::ReasoningLevel::Off,
        vec![structured_step(json!({ "grade": 3 }))],
    );
    let shape = br_llm_clients_core::StructuredShape::new(
        "graded",
        None,
        br_llm_clients_core::Parameters::new(vec![
            br_llm_clients_core::Parameter::new(
                "grade",
                "",
                true,
                br_llm_clients_core::ParameterKind::Integer,
            )
            .unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    let graph = retrying(
        &mock(vec![text_step("draft")]),
        CriticSeat {
            verdict: Some(VerdictOverride {
                shape,
                reader: Some(Arc::new(|_| -> Result<Verdict, GraphError> {
                    Err(GraphError::Structured {
                        message: "no scale".to_owned(),
                    })
                })),
            }),
            ..critic(&critic_client)
        },
        Count::Fixed(3),
    );

    let (_, error) = failed_in(call(&graph, question("explain")).await, "critique");

    assert!(
        matches!(error, GraphError::Structured { ref message } if message == "no scale"),
        "{error}"
    );
    assert_eq!(requests(&critic_client).len(), 1);
}
