//! Empty answers in what the critic reads: an empty or cut generation is
//! left out of its view, and the empty-answer critique falls back to its
//! default text when it resolves blank.

use std::collections::BTreeMap;

use br_llm_messages::{Text, WireMessage, WireUserBlock};

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::generator_critic_text_tests::{lax_generator, run_with, wrapped};
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::state::Value;

#[tokio::test]
async fn given_an_empty_answer_then_a_full_one_when_the_critic_judges_then_the_empty_one_is_left_out_of_its_view()
 {
    let critic_client = mock(vec![verdict(true, "")]);
    let graph = GeneratorCritic {
        empty_answer_critique: Some(Source::State(key("nudge"))),
        ..GeneratorCritic::new(
            lax_generator(vec![thinking_only_step(), text_step("full")]),
            critic(&critic_client),
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
        requests(&critic_client)[0].messages(),
        [
            merged,
            wrapped("full", "Please evaluate the generator's response above.")
        ]
    );
}

#[tokio::test]
async fn given_a_generation_cut_by_the_round_limit_then_a_full_one_when_the_critic_judges_then_the_cut_one_is_left_out_of_its_view()
 {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", serde_json::json!({})),
        tool_call_step("c2", "echo", serde_json::json!({})),
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
    ended(call(&graph, question("explain")).await, "validated");
    let merged = WireMessage::User {
        content: vec![
            WireUserBlock::Text {
                text: Text::new("<message role=\"user\">explain</message>").unwrap(),
            },
            WireUserBlock::Text {
                text: Text::new(
                    "<message author=\"reviewer\" role=\"agent\">The answer was empty. Write a complete answer.</message>",
                )
                .unwrap(),
            },
        ],
    };
    assert_eq!(
        requests(&critic_client)[0].messages(),
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
        let critic_client = mock(vec![verdict(true, "")]);
        let graph = GeneratorCritic {
            empty_answer_critique: Some(Source::State(key("nudge"))),
            ..GeneratorCritic::new(
                lax_generator(vec![thinking_only_step(), text_step("full")]),
                critic(&critic_client),
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
