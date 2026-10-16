//! The seats of the generator-critic built from functions of their
//! contexts: what each context holds, schedules that count critiques, tails
//! that take the last calls the budget allows, and the verdict shape that
//! follows the client of each critic call.

use std::sync::{Arc, Mutex};

use br_llm_clients_core::{OutputMode, ReasoningLevel};
use br_llm_messages::{Text, WireMessage};

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::*;
use crate::react::generator_critic_verdict_tests::critic_at;
use crate::react::given::{History, Model, Prompt, Reads};
use crate::react::helpers::wire;
use crate::react::rule_check::{RuleCheck, Ruling};
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::test_support::*;

#[tokio::test]
async fn given_seat_functions_when_run_then_the_generator_reads_its_generation_and_the_budget_and_the_critic_the_answer_it_judges()
 {
    let generator_client = mock(vec![text_step("first"), text_step("second")]);
    let critic_client = mock(vec![verdict(false, "More."), verdict(true, "")]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            system: Prompt::from_fn(Reads::new(), |ctx| {
                Ok(format!(
                    "Generation {} of a budget of {}, round {}, {} entries.",
                    ctx.generation(),
                    ctx.budget(),
                    ctx.round(),
                    ctx.conversation().entries().len()
                ))
            }),
            ..generator(&generator_client)
        },
        CriticSeat {
            review_request: Some(Prompt::from_fn(Reads::new(), |ctx| {
                Ok(format!("Judge generation {}.", ctx.generation()))
            })),
            ..CriticSeat::new(
                reviewer(),
                Model::client(Arc::new(critic_client.clone())),
                Prompt::from_fn(Reads::new(), |ctx| {
                    Ok(format!(
                        "Judge {:?}, attempt {}, budget {}.",
                        ctx.answer(),
                        ctx.attempt(),
                        ctx.budget()
                    ))
                }),
            )
        },
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    ended(call(&graph, question("explain")).await, "validated");

    let systems: Vec<Option<String>> = requests(&generator_client).iter().map(system_of).collect();
    assert_eq!(
        systems,
        [
            Some("Generation 1 of a budget of 2, round 0, 1 entries.".to_owned()),
            Some("Generation 2 of a budget of 2, round 0, 3 entries.".to_owned()),
        ]
    );
    let critic_requests = requests(&critic_client);
    let systems: Vec<Option<String>> = critic_requests.iter().map(system_of).collect();
    assert_eq!(
        systems,
        [
            Some("Judge \"first\", attempt 1, budget 2.".to_owned()),
            Some("Judge \"second\", attempt 1, budget 2.".to_owned()),
        ]
    );
    assert_eq!(
        critic_requests[1].messages().last(),
        Some(&user_message(
            "<generator_response>\nsecond\n</generator_response>\n\nJudge generation 2."
        ))
    );
}

#[tokio::test]
async fn given_a_generator_history_function_when_run_then_it_shapes_the_request_and_the_generation_is_recorded()
 {
    let generator_client = mock(vec![text_step("brief")]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            history: Some(History::from_fn(Reads::new(), |ctx| {
                let mut messages = wire(ctx.conversation(), &writer())?;
                messages.push(WireMessage::Relay {
                    content: vec![Text::new("Answer in one word.")?],
                });
                Ok(messages)
            })),
            ..generator(&generator_client)
        },
        critic(&mock(vec![verdict(true, "")])),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert!(matches!(
        requests(&generator_client)[0].messages().last(),
        Some(WireMessage::Relay { .. })
    ));
    assert_eq!(
        shape(state.transcript()),
        ["user", "writer:brief", "reviewer:Accepted."]
    );
}

#[tokio::test]
async fn given_a_generator_schedule_when_generations_are_revised_and_rejected_then_it_counts_the_critiques_only()
 {
    let first = mock(vec![text_step("a"), text_step("long answer")]);
    let then = mock(vec![text_step("another long answer")]);
    let critic_client = mock(vec![verdict(false, "Again."), verdict(true, "")]);
    let graph = GeneratorCritic {
        rule_check: Some(RuleCheck::from_fn(Reads::new(), |ctx| {
            let text: String = ctx.answer_step().text().map(Text::as_str).collect();
            Ok(if text.len() < 3 {
                Ruling::Revise(Text::new("Longer.")?)
            } else {
                Ruling::Pass
            })
        })),
        ..GeneratorCritic::new(
            GeneratorSeat {
                model: Model::schedule(Arc::new(first.clone()), vec![Arc::new(then.clone())]),
                ..generator(&first)
            },
            critic(&critic_client),
            Count::Fixed(2),
        )
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(state.int(&key("nb_generations")).unwrap(), 3);
    assert_eq!(
        (first.calls(), then.calls()),
        (2, 1),
        "the revised generation and its revision on the first client, the one after the critique on the next"
    );
}

#[tokio::test]
async fn given_tails_on_both_seats_when_run_then_each_takes_the_last_calls_the_budget_allows() {
    // A budget of 1 with a final generation allows two generations and one
    // critic call: the generator's tail takes the second, the critic's the
    // only one.
    let primary = mock(vec![text_step("draft")]);
    let tail = mock(vec![text_step("final")]);
    let critic_primary = mock(Vec::new());
    let critic_tail = mock(vec![verdict(false, "Fix it.")]);
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model: Model::ending_with(Arc::new(primary.clone()), vec![Arc::new(tail.clone())]),
            ..generator(&primary)
        },
        CriticSeat::new(
            reviewer(),
            Model::ending_with(
                Arc::new(critic_primary.clone()),
                vec![Arc::new(critic_tail.clone())],
            ),
            Prompt::text("Judge the answer."),
        ),
        Count::Fixed(1),
    )
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "exhausted");

    assert_eq!(state.str(&key("answer")).unwrap(), "final");
    assert_eq!((primary.calls(), tail.calls()), (1, 1));
    assert_eq!((critic_primary.calls(), critic_tail.calls()), (0, 1));

    // Without a final generation, a budget of 2 allows two generations and
    // two critic calls: each tail takes the second.
    let primary = mock(vec![text_step("draft")]);
    let tail = mock(vec![text_step("second")]);
    let critic_primary = mock(vec![verdict(false, "Fix it.")]);
    let critic_tail = mock(vec![verdict(false, "Still not.")]);
    let graph = GeneratorCritic {
        final_generation: false,
        ..GeneratorCritic::new(
            GeneratorSeat {
                model: Model::ending_with(Arc::new(primary.clone()), vec![Arc::new(tail.clone())]),
                ..generator(&primary)
            },
            CriticSeat::new(
                reviewer(),
                Model::ending_with(
                    Arc::new(critic_primary.clone()),
                    vec![Arc::new(critic_tail.clone())],
                ),
                Prompt::text("Judge the answer."),
            ),
            Count::Fixed(2),
        )
    }
    .graph()
    .unwrap();

    ended(call(&graph, question("explain")).await, "exhausted");

    assert_eq!((primary.calls(), tail.calls()), (1, 1));
    assert_eq!((critic_primary.calls(), critic_tail.calls()), (1, 1));
}

#[tokio::test]
async fn given_a_critic_schedule_across_reasoning_levels_when_it_judges_then_the_shape_asks_thinking_of_each_call_whose_client_does_not_reason()
 {
    let off = critic_at(ReasoningLevel::Off, vec![verdict(false, "More.")]);
    let high = critic_at(ReasoningLevel::High, vec![verdict(true, "")]);
    let graph = GeneratorCritic::new(
        generator(&mock(vec![text_step("a"), text_step("b")])),
        CriticSeat::new(
            reviewer(),
            Model::schedule(Arc::new(off.clone()), vec![Arc::new(high.clone())]),
            Prompt::text("Judge the answer."),
        ),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    ended(call(&graph, question("explain")).await, "validated");

    let names = |client: &br_llm_clients_mock::MockClient| -> Vec<String> {
        match requests(client)[0].output() {
            OutputMode::Structured(shape) => shape
                .parameters()
                .iter()
                .map(|parameter| parameter.name().to_owned())
                .collect(),
            OutputMode::Plain => panic!("expected the verdict shape"),
        }
    };
    assert_eq!(names(&off), ["thinking", "is_valid", "message"]);
    assert_eq!(names(&high), ["is_valid", "message"]);
}

#[tokio::test]
async fn given_an_empty_answer_critique_function_when_an_empty_answer_is_rejected_then_it_reads_the_generation_and_the_answer()
 {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    let graph = GeneratorCritic {
        empty_answer_critique: Some(Prompt::from_fn(Reads::new(), move |ctx| {
            kept.lock()
                .unwrap()
                .push((ctx.generation(), ctx.answer_step().thinking().count()));
            Ok(format!("Generation {} was empty.", ctx.generation()))
        })),
        ..GeneratorCritic::new(
            crate::react::generator_critic_text_tests::lax_generator(vec![
                thinking_only_step(),
                text_step("full"),
            ]),
            critic(&mock(vec![verdict(true, "")])),
            Count::Fixed(2),
        )
    }
    .graph()
    .unwrap();

    let state = ended(call(&graph, question("explain")).await, "validated");

    assert_eq!(
        shape(state.transcript())[2],
        "reviewer:Generation 1 was empty."
    );
    assert_eq!(*seen.lock().unwrap(), [(1, 1)]);
}
