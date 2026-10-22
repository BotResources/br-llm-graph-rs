//! The generator-critic's generator and the replies it cannot use: its final
//! reply checked against its shape and asked for again within its reply
//! retries, inside the same generation: a retry is no generation, no
//! revision and no critique.

use std::sync::Arc;

use br_llm_messages::{Text, WireMessage};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::rule_check::Ruling;
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;

/// The reminder the generator sends by default after a reply refused for
/// `reason`.
fn reminder(reason: &str) -> WireMessage {
    user_message(&format!(
        "Your reply could not be used: {reason}. Reply again."
    ))
}

#[tokio::test]
async fn given_a_structured_generator_when_its_reply_does_not_fit_then_it_is_asked_again_inside_the_same_generation()
 {
    let generator_client = mock(vec![
        structured_step(json!({ "text": ["a", "list"] })),
        structured_step(json!({ "text": "Tides follow the moon." })),
    ]);
    let critic_client = mock(vec![verdict(true, "Good.")]);
    let graph = GeneratorCritic::new(
        drafting(&generator_client, Some(Count::Fixed(1))),
        critic(&critic_client),
        Count::Fixed(3),
    )
    .graph()
    .unwrap();

    let done = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(
        done.str(&key("answer")).unwrap(),
        r#"{"text":"Tides follow the moon."}"#
    );
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(
        shape(done.transcript()),
        ["user", "writer:", "reviewer:Good."],
        "one generation, its fitting reply alone, and the acceptance"
    );
    let sent = requests(&generator_client);
    assert_eq!(sent.len(), 2);
    let mut expected = sent[0].messages().to_vec();
    expected.push(reminder("`text` is not a string"));
    assert_eq!(sent[1].messages(), expected.as_slice());
    let judged = requests(&critic_client);
    assert!(
        format!("{:?}", judged[0].messages()).contains("Tides follow the moon."),
        "the critic judges the fitting reply"
    );
}

#[tokio::test]
async fn given_a_structured_generator_with_no_retry_when_its_reply_does_not_fit_then_the_run_fails_and_no_critic_is_called()
 {
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic::new(
        drafting(&mock(vec![structured_step(json!({}))]), None),
        critic(&critic_client),
        Count::Fixed(3),
    )
    .graph()
    .unwrap();

    let (conversation, error) = failed_in(call(&graph, question("explain")).await, "generate");

    assert!(
        matches!(error, GraphError::Structured { ref message } if message == "`text` is missing or null"),
        "{error}"
    );
    assert_eq!(shape(&conversation), ["user"]);
    assert_eq!(critic_client.calls(), 0);
}

#[tokio::test]
async fn given_a_rule_check_when_a_generator_reply_is_asked_again_then_the_retry_is_no_revision_and_no_generation()
 {
    let generator_client = mock(vec![
        structured_step(json!({ "text": 1 })),
        structured_step(json!({ "text": "short" })),
        structured_step(json!({ "text": "a longer draft" })),
    ]);
    let critic_client = mock(vec![verdict(true, "Good.")]);
    let graph = GeneratorCritic {
        rule_check: Some(crate::react::given::Given::from_fn(
            crate::react::given::Reads::new(),
            |ctx| {
                if ctx.attempt() != 1 {
                    return Err("the rule check judges no reply asked for again".into());
                }
                let short = ctx
                    .answer_step()
                    .structured()
                    .any(|value| value["text"] == "short");
                Ok(if short {
                    Ruling::Revise(Text::new("Say more.").unwrap())
                } else {
                    Ruling::Pass
                })
            },
        )),
        ..GeneratorCritic::new(
            drafting(&generator_client, Some(Count::Fixed(1))),
            critic(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();

    let done = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(done.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(
        shape(done.transcript()),
        [
            "user",
            "writer:",
            "reviewer:Say more.",
            "writer:",
            "reviewer:Good."
        ]
    );
    assert_eq!(generator_client.calls(), 3);
}

#[tokio::test]
async fn given_a_generator_model_function_when_a_reply_is_asked_again_then_it_reads_the_attempt() {
    let generator_client = mock(vec![
        structured_step(json!({})),
        structured_step(json!({ "text": "fine" })),
    ]);
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (client, attempts) = (generator_client.clone(), seen.clone());
    let model =
        crate::react::given::Model::from_fn(crate::react::given::Reads::new(), move |ctx| {
            attempts
                .lock()
                .unwrap()
                .push((ctx.generation(), ctx.attempt()));
            let client: Arc<dyn br_llm_clients_core::Client> = Arc::new(client.clone());
            Ok(client)
        });
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model,
            ..drafting(&generator_client, Some(Count::Fixed(2)))
        },
        critic(&mock(vec![verdict(true, "Good.")])),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();

    ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(*seen.lock().unwrap(), [(1, 1), (1, 2)]);
}

#[test]
fn given_generator_reply_retries_read_from_a_configuration_key_when_built_then_the_key_is_a_configuration_key_of_the_graph()
 {
    let graph = GeneratorCritic::new(
        drafting(
            &mock(Vec::new()),
            Some(Count::Config(key("generator_retries"))),
        ),
        critic(&mock(Vec::new())),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    assert_eq!(
        graph.schema().config.get(&key("generator_retries")),
        Some(&crate::state::Kind::Int)
    );
}
