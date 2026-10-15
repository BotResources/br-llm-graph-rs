//! Empty answers of the generator-critic: rejected without the critic where
//! the critic would judge them, whether the step holds nothing but thinking
//! or closes a generation the round limit cut.
//!
//! A conforming client never gives a step without text, tool call or
//! structured value: it refuses it (`ReplyUnusable`), and the generation
//! fails. A step holding only thinking comes from `LaxClient`, which serves
//! it as it is, or from a history the host gives back.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_messages::{Turn, TurnId};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::given::Prompt;
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::test_support::*;

/// The generator seat on a `LaxClient` serving `steps`, and that client.
fn lax_generator(steps: Vec<br_llm_messages::Step>) -> (GeneratorSeat, Arc<LaxClient>) {
    let client = Arc::new(LaxClient::new(steps));
    let seat = GeneratorSeat {
        client: client.clone(),
        ..generator(&mock(Vec::new()))
    };
    (seat, client)
}

#[tokio::test]
async fn given_an_empty_answer_then_a_valid_one_when_run_then_the_empty_one_is_rejected_without_the_critic()
 {
    let (seat, generator_client) = lax_generator(vec![thinking_only_step(), text_step("full")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(seat, critic(&critic_client), Count::Fixed(2))
        .graph()
        .unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    let expected = [
        "user".to_owned(),
        "writer:".to_owned(),
        empty_critique(),
        "writer:full".to_owned(),
    ];
    assert_eq!(shape_of(&state, "conversation"), expected);
    assert_eq!(shape_of(&state, "generator_history"), expected);
    assert_eq!(state.str(&key("answer")).unwrap(), "full");
    assert_eq!(requests(&critic_client).len(), 1);
    assert_eq!(generator_client.requests().len(), 2);
}

#[tokio::test]
async fn given_an_empty_answer_on_the_last_judged_generation_when_run_then_exhausted_on_the_empty_answer_critique()
 {
    let (seat, generator_client) = lax_generator(vec![thinking_only_step()]);
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(seat, critic(&critic_client), Count::Fixed(1))
    }
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert!(!state.bool(&key("is_validated")).unwrap());
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user".to_owned(), "writer:".to_owned(), empty_critique()]
    );
    assert!(requests(&critic_client).is_empty());
    assert_eq!(generator_client.requests().len(), 1);
}

#[tokio::test]
async fn given_an_empty_unassessed_final_generation_when_run_then_exhausted_without_a_critique() {
    let (seat, _) = lax_generator(vec![text_step("a"), thinking_only_step()]);
    let critic_client = mock(vec![verdict(false, "fix it")]);
    let graph = GeneratorCritic::new(seat, critic(&critic_client), Count::Fixed(1))
        .graph()
        .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user", "writer:a", "reviewer:fix it", "writer:"]
    );
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_a_generation_cut_by_the_round_limit_when_run_then_rejected_as_empty_without_the_critic()
 {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("full"),
    ]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        cut_generator(&generator_client),
        critic(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "validated");
    assert_eq!(
        shape_of(&state, "conversation"),
        [
            "user".to_owned(),
            format!("writer:{CUT}"),
            empty_critique(),
            "writer:full".to_owned()
        ]
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(state.str(&key("answer")).unwrap(), "full");
    assert!(state.bool(&key("limited")).unwrap());
    assert_eq!(requests(&critic_client).len(), 1);
}

#[tokio::test]
async fn given_a_last_generation_cut_by_the_round_limit_when_run_then_the_answer_is_empty() {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic::new(
        cut_generator(&generator_client),
        critic(&critic_client),
        Count::Fixed(0),
    )
    .graph()
    .unwrap();
    let state = ended(call(&graph, question("explain")).await, "exhausted");
    assert_eq!(
        shape_of(&state, "conversation"),
        ["user".to_owned(), format!("writer:{CUT}")]
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "");
    assert!(state.bool(&key("limited")).unwrap());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_a_reply_holding_only_thinking_from_a_conforming_client_when_the_generator_answers_then_model_reply_unusable()
 {
    let generator_client = mock(vec![thinking_only_step()]);
    let critic_client = mock(Vec::new());
    let graph = duo(&generator_client, &critic_client).graph().unwrap();
    let (state, error) = failed_in(call(&graph, question("explain")).await, "generate");
    assert!(
        matches!(error, GraphError::Model(ClientError::ReplyUnusable { .. })),
        "{error}"
    );
    assert_eq!(shape_of(&state, "conversation"), ["user"]);
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_histories_ending_on_an_answer_holding_only_thinking_when_called_again_then_it_is_rejected_without_the_critic()
 {
    // A conforming client on both seats: the empty answer is a stored one.
    let mut conversation = question("explain");
    conversation
        .push_turn(Turn::new(
            TurnId::new("g1").unwrap(),
            Some(writer()),
            thinking_only_step(),
        ))
        .unwrap();
    let generator_client = mock(vec![text_step("full")]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        generator(&generator_client),
        critic(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    let state = ended(
        call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
        "validated",
    );

    let expected = [
        "user".to_owned(),
        "writer:".to_owned(),
        empty_critique(),
        "writer:full".to_owned(),
    ];
    assert_eq!(shape_of(&state, "conversation"), expected);
    assert_eq!(shape_of(&state, "generator_history"), expected);
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(state.str(&key("answer")).unwrap(), "full");
    assert_eq!(
        requests(&critic_client).len(),
        1,
        "the critic judges only the full answer"
    );
    assert_eq!(requests(&generator_client).len(), 1);
}

#[tokio::test]
async fn given_a_blank_critic_system_text_when_an_empty_answer_is_rejected_without_the_critic_then_the_run_ends_exhausted()
 {
    // The critic's system text is checked only when the critic is called:
    // an empty answer is rejected without it.
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            cut_generator(&generator_client),
            CriticSeat::new(
                reviewer(),
                Arc::new(critic_client.clone()),
                Prompt::text(" "),
            ),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "exhausted");

    assert_eq!(
        shape_of(&state, "conversation"),
        ["user".to_owned(), format!("writer:{CUT}"), empty_critique()]
    );
    assert_eq!(critic_client.calls(), 0);
}
