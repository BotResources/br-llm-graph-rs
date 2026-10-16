//! A durable generator-critic resumed after a crash: it routes on the turns
//! of the call the thread holds, exactly as the uninterrupted call would; a
//! generation the limit cut is still told from a provider's reply once read
//! back without its model.

use std::sync::Arc;

use br_llm_messages::{AssistantBlock, Step, StopReason, Text};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::*;
use crate::react::generator_critic_support::*;
use crate::react::given::Reads;
use crate::react::react_durable_support::{Host, nth_thread, opening, said};
use crate::react::rule_check::{RuleCheck, Ruling};
use crate::react::test_support::*;
use crate::run::{Outcome, control};
use crate::testkit::{Op, TestStore};

/// A rule check asking for a revision of the answer `short`.
fn no_short_answer() -> RuleCheck {
    RuleCheck::from_fn(Reads::new(), |ctx| {
        let text: String = ctx.answer_step().text().map(Text::as_str).collect();
        Ok(if text == "short" {
            Ruling::Revise(Text::new("Write more.")?)
        } else {
            Ruling::Pass
        })
    })
}

#[tokio::test]
async fn given_a_crash_during_the_critics_call_when_the_call_resumes_then_the_critic_judges_the_generation_the_thread_holds()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(Vec::new());
    unavailable(&critic_client);
    push(&critic_client, verdict(true, ""));
    let graph = durable_duo(&generator_client, &critic_client, &store)
        .graph()
        .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "explain")]);

    let (_, error) = failed_in(
        call_on(&host, &graph, question.clone(), "", "r1").await,
        "critique",
    );
    assert!(matches!(error, GraphError::Model(_)), "{error}");

    let thread = nth_thread(1);
    let done = ended(
        call_on(&host, &graph, question, thread.as_str(), "r1").await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "draft");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!((generator_client.calls(), critic_client.calls()), (1, 2));
    assert_eq!(
        shape(&store.view(&thread).unwrap().conversation),
        ["user", "writer:draft", "reviewer:Accepted."]
    );
    let inputs = writes(&store)
        .iter()
        .filter(|line| *line == "input i1")
        .count();
    assert_eq!(inputs, 1, "a restart never stores an input twice");
}

#[tokio::test]
async fn given_a_call_the_thread_holds_finished_when_it_runs_again_then_it_gives_its_outputs_without_a_model_call()
 {
    for (budget, label) in [
        (Count::Fixed(1), "validated"),
        (Count::Fixed(0), "exhausted"),
    ] {
        let store = Arc::new(TestStore::new());
        let generator_client = mock(vec![text_step("draft")]);
        let critic_client = mock(vec![verdict(true, "")]);
        let graph = GeneratorCritic {
            max_critiques: budget,
            ..durable_duo(&generator_client, &critic_client, &store)
        }
        .graph()
        .unwrap();
        let host = Host::new();
        let question = opening(&[said("i1", "explain")]);
        let first = ended(
            call_on(&host, &graph, question.clone(), "", "r1").await,
            label,
        );
        let calls = (generator_client.calls(), critic_client.calls());
        let written = writes(&store).len();

        let again = ended(
            call_on(&host, &graph, question, nth_thread(1).as_str(), "r1").await,
            label,
        );

        assert_eq!((generator_client.calls(), critic_client.calls()), calls);
        assert_eq!(
            again.str(&key("answer")).unwrap(),
            first.str(&key("answer")).unwrap()
        );
        assert_eq!(
            again.int(&key("nb_generations")).unwrap(),
            first.int(&key("nb_generations")).unwrap()
        );
        assert_eq!(
            again.bool(&key("is_validated")).unwrap(),
            first.bool(&key("is_validated")).unwrap()
        );
        assert_eq!(writes(&store).len(), written, "nothing written again");
    }
}

#[tokio::test]
async fn given_a_revision_found_on_entry_when_the_call_resumes_then_the_generator_answers_again_whatever_the_budget()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(vec![text_step("short")]);
    unavailable(&generator_client);
    push(&generator_client, text_step("long enough"));
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic {
        rule_check: Some(no_short_answer()),
        max_critiques: Count::Fixed(0),
        ..durable_duo(&generator_client, &critic_client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "explain")]);

    failed_in(
        call_on(&host, &graph, question.clone(), "", "r1").await,
        "generate",
    );
    let done = ended(
        call_on(&host, &graph, question, nth_thread(1).as_str(), "r1").await,
        "exhausted",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "long enough");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 2);
    assert_eq!(generator_client.calls(), 3);
    assert_eq!(critic_client.calls(), 0);
    assert_eq!(
        shape(&store.view(&nth_thread(1)).unwrap().conversation),
        [
            "user",
            "writer:short",
            "reviewer:Write more.",
            "writer:long enough"
        ],
        "the turn the crash left open, without a step, is not in the conversation"
    );
}

#[tokio::test]
async fn given_a_critique_found_on_entry_when_the_call_resumes_then_it_is_routed_as_after_the_rejection()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(vec![text_step("a")]);
    unavailable(&generator_client);
    push(&generator_client, text_step("b"));
    let critic_client = mock(vec![verdict(false, "fix it")]);
    let graph = GeneratorCritic {
        max_critiques: Count::Fixed(1),
        ..durable_duo(&generator_client, &critic_client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "explain")]);

    failed_in(
        call_on(&host, &graph, question.clone(), "", "r1").await,
        "generate",
    );
    let done = ended(
        call_on(&host, &graph, question, nth_thread(1).as_str(), "r1").await,
        "exhausted",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "b");
    assert_eq!(
        critic_client.calls(),
        1,
        "the final generation is unassessed"
    );
}

/// A complete answer `text` of a provider that keeps the stop reason the
/// round limit closes a cut generation with.
fn lookalike(text: &str) -> Step {
    Step::new(
        vec![AssistantBlock::Text {
            text: Text::new(text).unwrap(),
        }],
        StopReason::Other {
            reason: "tool_round_limit".to_owned(),
        },
        None,
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn given_a_provider_reply_with_the_limits_stop_reason_read_back_from_the_thread_when_the_call_resumes_then_it_is_judged()
 {
    // The thread gives the step back without its model: only the refusals
    // the limit writes before its closing step tell a cut generation, even
    // from a reply that holds the closing text itself.
    for text in [
        "A real, complete answer.",
        "Tool round limit reached (1): no final reply.",
    ] {
        let store = Arc::new(TestStore::new());
        let generator_client = mock(vec![lookalike(text)]);
        let critic_client = mock(Vec::new());
        unavailable(&critic_client);
        push(&critic_client, verdict(true, ""));
        let graph = GeneratorCritic {
            generator: cut_generator(&generator_client),
            ..durable_duo(&generator_client, &critic_client, &store)
        }
        .graph()
        .unwrap();
        let host = Host::new();
        let question = opening(&[said("i1", "explain")]);

        failed_in(
            call_on(&host, &graph, question.clone(), "", "r1").await,
            "critique",
        );
        let stored = store.view(&nth_thread(1)).unwrap();
        let br_llm_messages::Entry::Turn(generation) = &stored.conversation.entries()[1] else {
            panic!("expected the generation");
        };
        assert_eq!(
            crate::react::helpers::final_step(generation)
                .unwrap()
                .model(),
            None
        );

        let done = ended(
            call_on(&host, &graph, question, nth_thread(1).as_str(), "r1").await,
            "validated",
        );

        assert_eq!(done.str(&key("answer")).unwrap(), text);
        assert_eq!(critic_client.calls(), 2, "{text}");
    }
}

#[tokio::test]
async fn given_a_generation_the_limit_cut_found_on_entry_when_the_call_resumes_then_it_is_still_cut_and_the_flag_is_set()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
    ]);
    let critic_client = mock(Vec::new());
    let graph = GeneratorCritic {
        generator: cut_generator(&generator_client),
        max_critiques: Count::Fixed(0),
        ..durable_duo(&generator_client, &critic_client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "explain")]);
    let (sender, mut channel) = control();
    // The limit node completes the cut turn, then the run is cancelled before
    // its superstep is applied: the flag it set is lost.
    store.after(Op::EndTurn, 1, move || sender.cancel());

    let cancelled = call_listening(&host, &graph, question.clone(), "", "r1", &mut channel).await;
    assert!(matches!(cancelled.result, Ok(Outcome::Cancelled { .. })));

    let done = ended(
        call_on(&host, &graph, question, nth_thread(1).as_str(), "r1").await,
        "exhausted",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "");
    assert!(done.bool(&key("limited")).unwrap());
    assert_eq!(generator_client.calls(), 2);
}
