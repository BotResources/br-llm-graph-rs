//! The critic's verdict: the default shape `verdict` and its `thinking`
//! parameter, which follows the reasoning level of the critic's client, the
//! descriptions, and a caller's override with or without a reader.

use std::collections::BTreeMap;

use br_llm_clients_core::{
    ClientRequest, OutputMode, ParameterKind, ReasoningLevel, StructuredShape,
};
use br_llm_clients_mock::MockClient;
use br_llm_messages::{Conversation, Step};
use serde_json::json;

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::seats::CriticSeat;
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::state::{Config, Kind, State, Value};
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
) -> (ClientRequest, State) {
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("draft")])),
        critic,
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    let mut inputs = vec![
        (
            key("conversation"),
            Value::conversation(question("explain")),
        ),
        (
            key("generator_history"),
            Value::conversation(Conversation::new()),
        ),
    ];
    inputs.extend(extra);
    let state = graph.start_state(inputs).unwrap();
    let config = Config::new(graph.schema(), config).unwrap();
    let (_sender, mut inbox) = crate::run::channel();
    let state = ended(
        crate::run::run(&graph, &config, state, None, &ctx(), &mut inbox).await,
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
fn given_a_thinking_description_on_a_state_key_when_built_then_it_is_an_input_only_for_a_critic_that_does_not_reason()
 {
    let built = |level: ReasoningLevel| {
        GeneratorCritic::new(
            generator(&mock(Vec::new())),
            CriticSeat {
                thinking_description: Some(Source::State(key("reasoning_rule"))),
                ..critic(&mock_at(level))
            },
            Count::Fixed(1),
        )
        .graph()
        .unwrap()
    };
    assert_eq!(
        built(ReasoningLevel::Off)
            .signature()
            .inputs
            .get(&key("reasoning_rule")),
        Some(&Kind::Str)
    );
    assert_eq!(
        built(ReasoningLevel::High)
            .signature()
            .inputs
            .get(&key("reasoning_rule")),
        None,
        "a description not in use is not declared"
    );
}

#[tokio::test]
async fn given_custom_descriptions_when_the_critic_judges_then_each_is_resolved_into_the_shape() {
    let client = critic_at(ReasoningLevel::Off, vec![verdict(true, "")]);
    let seat = CriticSeat {
        thinking_description: Some(Source::Const("Your reasoning".to_owned())),
        is_valid_description: Some(Source::State(key("pass_rule"))),
        message_description: Some(Source::Config(key("feedback_rule"))),
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
