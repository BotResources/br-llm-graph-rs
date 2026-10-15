//! Resumed calls of the generator-critic: histories given back by the host,
//! routed on how `conversation` ends.

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
async fn given_histories_ending_on_a_critique_with_a_budget_of_zero_when_called_again_then_exhausted_without_generating()
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
        call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "a");
    assert!(requests(&generator_client).is_empty());
}

#[tokio::test]
async fn given_histories_ending_on_the_last_allowed_rejection_without_a_final_generation_when_called_again_then_exhausted_without_generating()
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
        call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "a");
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_histories_ending_on_a_turn_that_is_not_the_critics_when_called_again_then_the_generator_answers()
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
            call_with(&graph, conversation.clone(), conversation, BTreeMap::new()).await,
            "exhausted",
        );
        assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
        assert_eq!(state.str(&key("answer")).unwrap(), "b");
        assert_eq!(requests(&generator_client).len(), 1);
        assert!(requests(&critic_client).is_empty());
    }
}

#[tokio::test]
async fn given_histories_over_a_budget_read_smaller_when_called_again_then_exhausted_without_generating()
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
        call_with(&graph, conversation.clone(), conversation, config).await,
        "exhausted",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(state.str(&key("answer")).unwrap(), "b");
    assert!(requests(&generator_client).is_empty());
    assert!(requests(&critic_client).is_empty());
}

#[tokio::test]
async fn given_both_histories_ending_on_an_answer_when_called_again_then_the_critic_judges_it_first()
 {
    let mut conversation = question("explain");
    conversation
        .push_turn(Turn::new(
            TurnId::new("g1").unwrap(),
            Some(writer()),
            text_step("saved answer"),
        ))
        .unwrap();
    let mut generator_history = question("explain");
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
    generator_history.push_turn(turn).unwrap();

    let generator_client = mock(Vec::new());
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = duo(&generator_client, &critic_client).graph().unwrap();
    let state = ended(
        call_with(&graph, conversation, generator_history, BTreeMap::new()).await,
        "validated",
    );
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(state.str(&key("answer")).unwrap(), "saved answer");
    assert_eq!(
        shape_of(&state, "generator_history"),
        ["user", "writer:[3 items]"]
    );
    assert!(requests(&generator_client).is_empty());
}
