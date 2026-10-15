//! The texts of the generator-critic: every text is a source, resolved at the
//! call, and what the critic reads around the answer it judges.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_messages::{Conversation, Step, WireMessage};

use crate::error::GraphError;
use crate::graph::{Count, Graph};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, Kind, Value};
use crate::value::Key;

/// A new call on `question("explain")` with the given extra inputs and
/// configuration.
pub(crate) async fn run_with(
    graph: &Graph,
    extra: Vec<(Key, Value)>,
    config: BTreeMap<Key, Value>,
) -> Result<Outcome, RunFailure> {
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
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx(), &mut inbox).await
}

pub(crate) fn wrapped(answer: &str, request: &str) -> WireMessage {
    let mut text = format!("<generator_response>\n{answer}\n</generator_response>");
    if !request.is_empty() {
        text.push_str("\n\n");
        text.push_str(request);
    }
    user_message(&text)
}

pub(crate) fn opening() -> WireMessage {
    user_message("<message role=\"user\">explain</message>")
}

/// The generator seat on a `LaxClient` serving `steps`: a step holding only
/// thinking, which a conforming client refuses, reaches the graph.
pub(crate) fn lax_generator(steps: Vec<Step>) -> GeneratorSeat {
    GeneratorSeat {
        client: Arc::new(LaxClient::new(steps)),
        ..generator(&mock(Vec::new()))
    }
}

#[tokio::test]
async fn given_a_custom_or_a_blank_review_request_when_the_critic_judges_then_it_follows_the_wrapped_answer()
 {
    for (request, expected) in [("Is it correct?", "Is it correct?"), ("", ""), ("  \n", "")] {
        let generator_client = mock(vec![text_step("draft")]);
        let critic_client = mock(vec![verdict(true, "")]);
        let graph = GeneratorCritic::new(
            generator(&generator_client),
            CriticSeat {
                review_request: Some(Source::Const(request.to_owned())),
                ..critic(&critic_client)
            },
            Count::Fixed(1),
        )
        .graph()
        .unwrap();
        ended(call(&graph, question("explain")).await, "validated");
        assert_eq!(
            requests(&critic_client)[0].messages(),
            [opening(), wrapped("draft", expected)],
            "{request:?}"
        );
    }
}

#[tokio::test]
async fn given_texts_from_every_kind_of_source_when_run_then_each_is_resolved_at_the_call() {
    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let system = Source::Fn(Arc::new(|state, config| {
        Ok(format!(
            "Write about {} in a {} tone.",
            state.str(&key("topic"))?,
            config.str(&key("tone"))?
        ))
    }));
    let graph = GeneratorCritic {
        inputs: vec![(key("topic"), Kind::Str)],
        ..GeneratorCritic::new(
            GeneratorSeat {
                system,
                ..generator(&generator_client)
            },
            CriticSeat {
                review_request: Some(Source::State(key("ask"))),
                ..CriticSeat::new(
                    reviewer(),
                    Arc::new(critic_client.clone()),
                    Source::Config(key("tone")),
                )
            },
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    assert_eq!(graph.signature().inputs.get(&key("ask")), Some(&Kind::Str));
    assert_eq!(graph.schema().config.get(&key("tone")), Some(&Kind::Str));
    ended(
        run_with(
            &graph,
            vec![
                (key("topic"), Value::str("tides")),
                (key("ask"), Value::str("Grade it.")),
            ],
            BTreeMap::from([(key("tone"), Value::str("dry"))]),
        )
        .await,
        "validated",
    );
    assert_eq!(
        system_of(&requests(&generator_client)[0]).as_deref(),
        Some("Write about tides in a dry tone.")
    );
    let request = &requests(&critic_client)[0];
    assert_eq!(system_of(request).as_deref(), Some("dry"));
    assert_eq!(
        request.messages(),
        [opening(), wrapped("draft", "Grade it.")]
    );
}

#[test]
fn given_one_state_key_read_by_both_seats_when_built_then_one_string_input() {
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            system: Source::State(key("brief")),
            ..generator(&mock(Vec::new()))
        },
        CriticSeat::new(
            reviewer(),
            Arc::new(mock(Vec::new())),
            Source::State(key("brief")),
        ),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("brief")),
        Some(&Kind::Str)
    );
}

#[test]
fn given_a_source_key_that_is_already_a_key_of_the_graph_when_built_then_key_collision() {
    let state = GeneratorCritic::new(
        generator(&mock(Vec::new())),
        CriticSeat::new(
            reviewer(),
            Arc::new(mock(Vec::new())),
            Source::State(key("conversation")),
        ),
        Count::Fixed(1),
    )
    .graph();
    assert!(matches!(
        state,
        Err(GraphError::KeyCollision { key: ref found }) if *found == key("conversation")
    ));

    let config = GeneratorCritic::new(
        generator(&mock(Vec::new())),
        CriticSeat::new(
            reviewer(),
            Arc::new(mock(Vec::new())),
            Source::Config(key("rounds")),
        ),
        Count::Config(key("rounds")),
    )
    .graph();
    assert!(matches!(
        config,
        Err(GraphError::KeyCollision { key: ref found }) if *found == key("rounds")
    ));
}

#[tokio::test]
async fn given_a_critic_system_text_that_resolves_blank_when_the_critic_judges_then_no_system_text()
{
    for system in ["", " \n"] {
        let critic_client = mock(vec![verdict(true, "")]);
        let graph = GeneratorCritic::new(
            generator(&mock(vec![text_step("draft")])),
            CriticSeat::new(
                reviewer(),
                Arc::new(critic_client.clone()),
                Source::Const(system.to_owned()),
            ),
            Count::Fixed(1),
        )
        .graph()
        .unwrap();
        ended(call(&graph, question("explain")).await, "validated");
        let request = &requests(&critic_client)[0];
        assert_eq!(system_of(request), None, "{system:?}");
        assert_eq!(
            request.messages(),
            [
                opening(),
                wrapped("draft", "Please evaluate the generator's response above.")
            ]
        );
    }
}
