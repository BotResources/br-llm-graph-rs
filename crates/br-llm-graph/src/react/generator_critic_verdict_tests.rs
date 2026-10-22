//! The critic's verdict: the default shape `verdict` and its `thinking`
//! parameter, which follows the reasoning level of the critic's client, its
//! descriptions, and how a reply is read as a verdict.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{
    ClientError, ClientRequest, OutputMode, ParameterKind, ReasoningLevel, StructuredShape,
};
use br_llm_clients_mock::MockClient;
use br_llm_messages::Step;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::given::{Model, Prompt};
use crate::react::seats::CriticSeat;
use crate::react::test_support::*;
use crate::state::{Kind, Value};
use crate::value::Key;

/// A critic client at `reasoning` whose script is `replies`.
pub(crate) fn critic_at(reasoning: ReasoningLevel, replies: Vec<Step>) -> MockClient {
    let client = mock_at(reasoning);
    for reply in replies {
        push(&client, reply);
    }
    client
}

/// The critic's request on a one-round run on `question("explain")` whose
/// answer `critic` validates, with `extra` inputs and `config`, and the state
/// at its end. `critic_client` is the client `critic` sits on.
pub(crate) async fn judged_with(
    critic: CriticSeat,
    critic_client: &MockClient,
    extra: Vec<(Key, Value)>,
    config: BTreeMap<Key, Value>,
) -> (ClientRequest, Done) {
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("draft")])),
        critic,
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let state = ended(
        call_with(&graph, question("explain"), extra, config).await,
        "validated",
    );
    (requests(critic_client).remove(0), state)
}

pub(crate) fn shape_sent(request: &ClientRequest) -> &StructuredShape {
    match request.output() {
        OutputMode::Structured(shape) => shape,
        OutputMode::Plain => panic!("expected a structured request"),
    }
}

/// Each parameter of `shape` as `(name, kind, required, description)`.
fn parameters_of(shape: &StructuredShape) -> Vec<(&str, ParameterKind, bool, &str)> {
    shape
        .parameters()
        .iter()
        .map(|parameter| {
            (
                parameter.name(),
                parameter.kind().clone(),
                parameter.is_required(),
                parameter.description(),
            )
        })
        .collect()
}

const THINKING: &str = "Explanation of the thought process and validation steps";
const IS_VALID: &str = "Boolean indicating if the generated message is valid";
const MESSAGE: &str = "Feedback message for the generator, empty if validation passed";

#[tokio::test]
async fn given_a_critic_that_does_not_reason_when_it_judges_then_the_verdict_shape_asks_thinking_is_valid_and_message_in_order()
 {
    let client = critic_at(ReasoningLevel::Off, vec![verdict(true, "")]);
    let (request, _) = judged_with(critic(&client), &client, Vec::new(), BTreeMap::new()).await;
    let shape = shape_sent(&request);
    assert_eq!(shape.name(), "verdict");
    assert_eq!(shape.description(), None);
    assert_eq!(
        parameters_of(shape),
        [
            ("thinking", ParameterKind::String, true, THINKING),
            ("is_valid", ParameterKind::Boolean, true, IS_VALID),
            ("message", ParameterKind::String, true, MESSAGE),
        ]
    );
    assert!(request.tools().is_empty());
}

#[tokio::test]
async fn given_a_critic_that_reasons_natively_when_it_judges_then_the_verdict_shape_has_no_thinking()
 {
    for level in [
        ReasoningLevel::Low,
        ReasoningLevel::Medium,
        ReasoningLevel::High,
        ReasoningLevel::Extra,
        ReasoningLevel::Max,
    ] {
        let client = critic_at(
            level,
            vec![structured_step(json!({ "is_valid": true, "message": "" }))],
        );
        let (request, _) = judged_with(critic(&client), &client, Vec::new(), BTreeMap::new()).await;
        let shape = shape_sent(&request);
        assert_eq!(shape.name(), "verdict");
        assert_eq!(
            parameters_of(shape),
            [
                ("is_valid", ParameterKind::Boolean, true, IS_VALID),
                ("message", ParameterKind::String, true, MESSAGE),
            ],
            "{level:?}"
        );
    }
}

#[test]
fn given_a_thinking_description_on_a_state_key_when_built_then_it_is_an_input_whatever_the_critic_reasons()
 {
    // Whether a call asks for `thinking` is known only at the call, from the
    // client the critic's model gives then: the description's key is an
    // input in any case.
    let built = |level: ReasoningLevel| {
        GeneratorCritic::new(
            generator(&mock(Vec::new())),
            CriticSeat {
                thinking_description: Some(Prompt::from_state(&skey("reasoning_rule"))),
                ..critic(&mock_at(level))
            },
            Count::Fixed(1),
        )
        .graph()
        .unwrap()
    };
    for level in [ReasoningLevel::Off, ReasoningLevel::High] {
        assert_eq!(
            built(level).signature().inputs.get(&key("reasoning_rule")),
            Some(&Kind::Str),
            "{level:?}"
        );
    }
}

#[tokio::test]
async fn given_custom_descriptions_when_the_critic_judges_then_each_is_resolved_into_the_shape() {
    let client = critic_at(ReasoningLevel::Off, vec![verdict(true, "")]);
    let seat = CriticSeat {
        thinking_description: Some(Prompt::text("Your reasoning")),
        is_valid_description: Some(Prompt::from_state(&skey("pass_rule"))),
        message_description: Some(Prompt::from_config(&ckey("feedback_rule"))),
        ..critic(&client)
    };
    let (request, _) = judged_with(
        seat,
        &client,
        vec![(key("pass_rule"), Value::str("True when complete"))],
        BTreeMap::from([(key("feedback_rule"), Value::str("What to change"))]),
    )
    .await;
    assert_eq!(
        parameters_of(shape_sent(&request)),
        [
            ("thinking", ParameterKind::String, true, "Your reasoning"),
            (
                "is_valid",
                ParameterKind::Boolean,
                true,
                "True when complete"
            ),
            ("message", ParameterKind::String, true, "What to change"),
        ]
    );
}

#[tokio::test]
async fn given_a_block_that_does_not_read_as_the_default_verdict_when_the_critic_judges_then_structured_and_no_critique()
 {
    for malformed in [
        structured_step(json!({ "thinking": "hm", "is_valid": "yes", "message": "x" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false, "message": "" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false, "message": " \n" })),
        structured_step(json!({ "thinking": "hm", "is_valid": false })),
    ] {
        let generator_client = mock(vec![text_step("draft")]);
        let critic_client = mock(vec![malformed]);
        let graph = duo(&generator_client, &critic_client).graph().unwrap();
        let (conversation, error) = failed_in(call(&graph, question("explain")).await, "critique");
        assert!(matches!(error, GraphError::Structured { .. }), "{error}");
        assert_eq!(shape(&conversation), ["user", "writer:draft"]);
    }
}

#[tokio::test]
async fn given_a_critic_step_without_a_structured_block_when_the_critic_judges_then_structured_and_no_critique()
 {
    // A conforming client refuses such a reply itself (next test): the step
    // comes from a client that serves it as it is.
    let critic_client = Arc::new(LaxClient::new(vec![text_step("looks fine to me")]));
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("draft")])),
        CriticSeat {
            model: Model::client(critic_client.clone()),
            ..critic(&mock(Vec::new()))
        },
        Count::Fixed(3),
    )
    .graph()
    .unwrap();
    let (conversation, error) = failed_in(call(&graph, question("explain")).await, "critique");
    assert!(
        matches!(error, GraphError::Structured { ref message } if message == "the step carries no structured block"),
        "{error}"
    );
    assert_eq!(critic_client.requests().len(), 1);
    assert_eq!(shape(&conversation), ["user", "writer:draft"]);
}

#[tokio::test]
async fn given_a_text_reply_to_the_verdict_request_from_a_conforming_client_then_an_unreadable_verdict_and_no_critique()
 {
    let critic_client = mock(vec![text_step("looks fine to me")]);
    let graph = duo(&mock(vec![text_step("draft")]), &critic_client)
        .graph()
        .unwrap();
    let (conversation, error) = failed_in(call(&graph, question("explain")).await, "critique");
    let refused = ClientError::MalformedResponse {
        detail: "the step does not fit a structured request".to_owned(),
    };
    assert!(
        matches!(error, GraphError::Structured { ref message } if *message == refused.to_string()),
        "{error}"
    );
    assert_eq!(shape(&conversation), ["user", "writer:draft"]);
}
