use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_messages::{Conversation, Text, WireMessage, WireUserBlock};

use crate::error::GraphError;
use crate::graph::{Count, Graph, Switch};
use crate::react::generator_critic::{CriticSeat, GeneratorCritic, GeneratorSeat};
use crate::react::generator_critic_tests::*;
use crate::react::llm_node::Source;
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, Kind, Value};
use crate::value::Key;

/// A new call on `question("explain")` with the given extra inputs and
/// configuration.
async fn run_with(
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

fn wrapped(answer: &str, request: &str) -> WireMessage {
    let mut text = format!("<generator_response>\n{answer}\n</generator_response>");
    if !request.is_empty() {
        text.push_str("\n\n");
        text.push_str(request);
    }
    user_message(&text)
}

fn opening() -> WireMessage {
    user_message("<message role=\"user\">explain</message>")
}

#[tokio::test]
async fn given_a_custom_or_a_blank_review_request_when_the_critic_judges_then_it_follows_the_wrapped_answer()
 {
    for (request, expected) in [("Is it correct?", "Is it correct?"), ("", ""), ("  \n", "")] {
        let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
        let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
        let graph = GeneratorCritic::new(
            generator(generator_model),
            CriticSeat {
                review_request: Some(Source::Const(request.to_owned())),
                ..critic(critic_model.clone())
            },
            Count::Fixed(1),
        )
        .graph()
        .unwrap();
        ended(call(&graph, question("explain")).await, "validated");
        assert_eq!(
            critic_model.requests()[0].messages,
            [opening(), wrapped("draft", expected)],
            "{request:?}"
        );
    }
}

#[tokio::test]
async fn given_texts_from_every_kind_of_source_when_run_then_each_is_resolved_at_the_call() {
    let generator_model = Arc::new(LoggingModel::new(vec![text_step("draft")]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
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
                ..generator(generator_model.clone())
            },
            CriticSeat {
                review_request: Some(Source::State(key("ask"))),
                ..CriticSeat::new(
                    reviewer(),
                    critic_model.clone(),
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
        generator_model.requests()[0].system.as_deref(),
        Some("Write about tides in a dry tone.")
    );
    let request = &critic_model.requests()[0];
    assert_eq!(request.system.as_deref(), Some("dry"));
    assert_eq!(request.messages, [opening(), wrapped("draft", "Grade it.")]);
}

#[test]
fn given_one_state_key_read_by_both_seats_when_built_then_one_string_input() {
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            system: Source::State(key("brief")),
            ..generator(Arc::new(LoggingModel::new(Vec::new())))
        },
        CriticSeat::new(
            reviewer(),
            Arc::new(LoggingModel::new(Vec::new())),
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
    let models = || {
        (
            Arc::new(LoggingModel::new(Vec::new())),
            Arc::new(LoggingModel::new(Vec::new())),
        )
    };
    let (generator_model, critic_model) = models();
    let state = GeneratorCritic::new(
        generator(generator_model),
        CriticSeat::new(reviewer(), critic_model, Source::State(key("conversation"))),
        Count::Fixed(1),
    )
    .graph();
    assert!(matches!(
        state,
        Err(GraphError::KeyCollision { key: ref found }) if *found == key("conversation")
    ));

    let (generator_model, critic_model) = models();
    let config = GeneratorCritic::new(
        generator(generator_model),
        CriticSeat {
            thinking: Some(Switch::Config(key("deep"))),
            ..CriticSeat::new(reviewer(), critic_model, Source::Config(key("deep")))
        },
        Count::Fixed(1),
    )
    .graph();
    assert!(matches!(
        config,
        Err(GraphError::KeyCollision { key: ref found }) if *found == key("deep")
    ));
}

#[tokio::test]
async fn given_an_empty_answer_then_a_full_one_when_the_critic_judges_then_the_empty_one_is_left_out_of_its_view()
 {
    let generator_model = Arc::new(LoggingModel::new(vec![empty_step(), text_step("full")]));
    let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
    let graph = GeneratorCritic {
        empty_answer_critique: Some(Source::State(key("nudge"))),
        ..GeneratorCritic::new(
            generator(generator_model),
            critic(critic_model.clone()),
            Count::Fixed(2),
        )
    }
    .graph()
    .unwrap();
    let state = ended(
        run_with(
            &graph,
            vec![(key("nudge"), Value::str("Say something."))],
            BTreeMap::new(),
        )
        .await,
        "validated",
    );
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user", "writer:", "reviewer:Say something.", "writer:full"]
    );
    let merged = WireMessage::User {
        content: vec![
            WireUserBlock::Text {
                text: Text::new("<message role=\"user\">explain</message>").unwrap(),
            },
            WireUserBlock::Text {
                text: Text::new(
                    "<message author=\"reviewer\" role=\"agent\">Say something.</message>",
                )
                .unwrap(),
            },
        ],
    };
    assert_eq!(
        critic_model.requests()[0].messages,
        [
            merged,
            wrapped("full", "Please evaluate the generator's response above.")
        ]
    );
}

#[tokio::test]
async fn given_an_empty_answer_critique_that_resolves_blank_when_an_empty_answer_is_rejected_then_the_default_text()
 {
    for nudge in ["", "  \n"] {
        let generator_model = Arc::new(LoggingModel::new(vec![empty_step(), text_step("full")]));
        let critic_model = Arc::new(LoggingModel::new(vec![verdict(true, "")]));
        let graph = GeneratorCritic {
            empty_answer_critique: Some(Source::State(key("nudge"))),
            ..GeneratorCritic::new(
                generator(generator_model),
                critic(critic_model),
                Count::Fixed(2),
            )
        }
        .graph()
        .unwrap();
        let state = ended(
            run_with(
                &graph,
                vec![(key("nudge"), Value::str(nudge))],
                BTreeMap::new(),
            )
            .await,
            "validated",
        );
        assert_eq!(
            shape_of(&state, "conversation"),
            [
                "user",
                "writer:",
                "reviewer:The answer was empty. Write a complete answer.",
                "writer:full"
            ]
        );
    }
}
