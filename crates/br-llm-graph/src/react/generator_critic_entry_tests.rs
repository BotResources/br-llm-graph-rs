//! Where a call of the generator-critic in memory enters: on the last turn
//! of the generator or the critic since the trigger, in its opening.

use std::collections::BTreeMap;

use br_llm_messages::{
    Author, Text, ToolCallId, ToolName, ToolResult, ToolResultBlock, Turn, TurnId,
};
use serde_json::json;

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::test_support::*;
use crate::state::Value;

#[tokio::test]
async fn given_an_opening_ending_on_a_critique_with_a_budget_of_zero_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [("g1", writer(), "a"), ("c1", reviewer(), "fix it")] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_client = mock(Vec::new());
    let graph = with_budget(&generator_client, &mock(Vec::new()), Count::Fixed(0), true);
    let state = ended(
        call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "a");
    assert!(requests(&generator_client).is_empty());
}

#[tokio::test]
async fn given_an_opening_ending_on_the_last_allowed_rejection_without_a_final_generation_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [("g1", writer(), "a"), ("c1", reviewer(), "fix it")] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_client = mock(Vec::new());
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            generator(&generator_client),
            critic(&critic_client),
            Count::Fixed(1),
        )
    }
    .graph()
    .unwrap();
    let state = ended(
        call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "a");
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_an_opening_ending_on_a_turn_of_another_author_when_called_then_it_is_routed_on_the_last_turn_of_a_seat()
 {
    let third = Author::new("observer", "observer").unwrap();
    for last in [Some(third), None] {
        let mut conversation = question("explain");
        for (id, author, text) in [
            ("g1", Some(writer()), "a"),
            ("c1", Some(reviewer()), "fix it"),
            ("x1", last, "a note"),
        ] {
            conversation
                .push_turn(Turn::new(TurnId::new(id).unwrap(), author, text_step(text)))
                .unwrap();
        }
        let generator_client = mock(vec![text_step("b")]);
        let critic_client = mock(Vec::new());
        let graph = GeneratorCritic {
            final_generation: false,
            ..GeneratorCritic::new(
                generator(&generator_client),
                critic(&critic_client),
                Count::Fixed(1),
            )
        }
        .graph()
        .unwrap();
        let state = ended(
            call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
            "exhausted",
        );
        assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
        assert_eq!(state.str(&key("answer")).unwrap(), "a");
        assert!(requests(&generator_client).is_empty());
        assert!(requests(&critic_client).is_empty());
    }
}

#[tokio::test]
async fn given_an_opening_over_a_budget_read_smaller_when_called_again_then_exhausted_without_generating()
 {
    let mut conversation = question("explain");
    for (id, author, text) in [
        ("g1", writer(), "a"),
        ("c1", reviewer(), "fix it"),
        ("g2", writer(), "b"),
        ("c2", reviewer(), "fix it again"),
    ] {
        conversation
            .push_turn(Turn::new(
                TurnId::new(id).unwrap(),
                Some(author),
                text_step(text),
            ))
            .unwrap();
    }
    let generator_client = mock(vec![text_step("c")]);
    let critic_client = mock(Vec::new());
    let graph = with_budget(
        &generator_client,
        &critic_client,
        Count::Config(key("critiques")),
        true,
    );
    let config = BTreeMap::from([(key("critiques"), Value::int(1))]);
    let state = ended(
        call_with(&graph, conversation, Vec::new(), config).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(state.str(&key("answer")).unwrap(), "b");
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_an_opening_ending_on_an_answer_when_called_then_the_critic_judges_it_first() {
    let mut conversation = question("explain");
    let mut turn = Turn::new(
        TurnId::new("g1").unwrap(),
        Some(writer()),
        tool_call_step("c1", "echo", json!({})),
    );
    turn.push_result(ToolResult::new(
        ToolCallId::new("c1").unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    ))
    .unwrap();
    turn.push_step(text_step("saved answer")).unwrap();
    conversation.push_turn(turn).unwrap();

    let generator_client = mock(Vec::new());
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = duo(&generator_client, &critic_client).graph().unwrap();
    let state = ended(
        call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
        "validated",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "saved answer");
    assert_eq!(
        shape(state.transcript()),
        [
            "user",
            "writer:[3 items] saved answer",
            "reviewer:Accepted."
        ]
    );
    assert!(requests(&generator_client).is_empty());
}

/// `question("explain")` followed by turns of `(id, author, step)`.
fn opening_with(
    turns: Vec<(&str, Author, br_llm_messages::Step)>,
) -> br_llm_messages::Conversation {
    let mut conversation = question("explain");
    for (id, author, step) in turns {
        conversation
            .push_turn(Turn::new(TurnId::new(id).unwrap(), Some(author), step))
            .unwrap();
    }
    conversation
}

/// A step of the critic's of the stop reason `other(reason)`.
fn message_of(text: &str, reason: &str) -> br_llm_messages::Step {
    br_llm_messages::Step::new(
        vec![br_llm_messages::AssistantBlock::Text {
            text: Text::new(text).unwrap(),
        }],
        br_llm_messages::StopReason::Other {
            reason: reason.to_owned(),
        },
        None,
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn given_an_opening_ending_on_an_acceptance_when_called_then_it_finishes_validated_without_a_model_call()
 {
    let conversation = opening_with(vec![
        ("g1", writer(), text_step("kept")),
        ("c1", reviewer(), message_of("Accepted.", "accepted")),
    ]);
    let generator_client = mock(Vec::new());
    let critic_client = mock(Vec::new());
    let graph = duo(&generator_client, &critic_client).graph().unwrap();

    let state = ended(
        call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
        "validated",
    );

    assert!(state.bool(&key("is_validated")).unwrap());
    assert_eq!(state.str(&key("answer")).unwrap(), "kept");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!((generator_client.calls(), critic_client.calls()), (0, 0));
}

#[tokio::test]
async fn given_an_opening_ending_on_a_revision_with_a_budget_of_zero_when_called_then_the_generator_answers_again()
 {
    let conversation = opening_with(vec![
        ("g1", writer(), text_step("a")),
        ("c1", reviewer(), message_of("Longer.", "rule_check")),
    ]);
    let generator_client = mock(vec![text_step("a longer answer")]);
    let critic_client = mock(Vec::new());
    let graph = with_budget(&generator_client, &critic_client, Count::Fixed(0), true);

    let state = ended(
        call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
        "exhausted",
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "a longer answer");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(critic_client.calls(), 0);
}
