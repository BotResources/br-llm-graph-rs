//! A revision found on entry at the revision limit: it is routed as the
//! check routed it, the revisions before it in a row counted. One the check
//! recorded sends the generator back, whatever the budget; one past the
//! limit is not one the check would record, so the generation before it
//! goes on as if it had passed.

use std::collections::BTreeMap;

use br_llm_messages::{AssistantBlock, Author, Conversation, Step, StopReason, Text, Turn, TurnId};

use crate::graph::Count;
use crate::react::generator_critic_support::*;
use crate::react::test_support::*;

/// A revision asked by the rule check, as the check records it.
fn revision(text: &str) -> Step {
    Step::new(
        vec![AssistantBlock::Text {
            text: Text::new(text).unwrap(),
        }],
        StopReason::Other {
            reason: "rule_check".to_owned(),
        },
        None,
        None,
    )
    .unwrap()
}

/// `question("explain")` followed by `turns`.
fn opening_with(turns: Vec<(&str, Author, Step)>) -> Conversation {
    let mut conversation = question("explain");
    for (id, author, step) in turns {
        conversation
            .push_turn(Turn::new(TurnId::new(id).unwrap(), Some(author), step))
            .unwrap();
    }
    conversation
}

#[tokio::test]
async fn given_an_opening_ending_on_the_second_revision_in_a_row_with_a_limit_of_two_when_called_then_the_generator_answers_again_whatever_the_budget()
 {
    let conversation = opening_with(vec![
        ("g1", writer(), text_step("a")),
        ("r1", reviewer(), revision("Longer.")),
        ("g2", writer(), text_step("aa")),
        ("r2", reviewer(), revision("Longer still.")),
    ]);
    let generator_client = mock(vec![text_step("aaa")]);
    let critic_client = mock(Vec::new());
    let graph = with_budget(&generator_client, &critic_client, Count::Fixed(0), true);

    let state = ended(
        call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
        "exhausted",
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "aaa");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 3);
    assert_eq!((generator_client.calls(), critic_client.calls()), (1, 0));
}

#[tokio::test]
async fn given_an_opening_ending_on_three_revisions_in_a_row_with_a_limit_of_two_when_called_then_the_generation_goes_on_as_if_it_had_passed()
 {
    let conversation = opening_with(vec![
        ("g1", writer(), text_step("a")),
        ("r1", reviewer(), revision("Longer.")),
        ("g2", writer(), text_step("aa")),
        ("r2", reviewer(), revision("Longer.")),
        ("g3", writer(), text_step("aaa")),
        ("r3", reviewer(), revision("Longer.")),
    ]);
    let generator_client = mock(Vec::new());
    let critic_client = mock(Vec::new());
    let graph = with_budget(&generator_client, &critic_client, Count::Fixed(0), true);

    let state = ended(
        call_with(&graph, conversation, Vec::new(), BTreeMap::new()).await,
        "exhausted",
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "aaa");
    assert_eq!(state.int(&key("nb_generations")).unwrap(), 3);
    assert_eq!((generator_client.calls(), critic_client.calls()), (0, 0));
}
