//! A durable generator-critic: its conversation is its thread, every
//! generation and message of the critic written there; a call resumed after
//! a crash routes on the turns of the call the thread holds, exactly as the
//! uninterrupted call would; it gives the same outputs and requests as the
//! same call in memory.

use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::{AssistantBlock, Step, StopReason, Text, Turn, TurnId};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::MessageStatus;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::*;
use crate::react::generator_critic_support::*;
use crate::react::given::Reads;
use crate::react::react_durable_support::{Host, nth_thread, opening, said};
use crate::react::rule_check::{RuleCheck, Ruling};
use crate::react::test_support::*;
use crate::run::{Outcome, control};
use crate::testkit::{Op, TestStore};

fn unavailable(client: &MockClient) {
    client.push_reply(
        Reply::failure(ClientError::Unavailable { retry_after: None })
            .build()
            .unwrap(),
    );
}

/// The writes of the store's log, reads left out.
fn writes(store: &TestStore) -> Vec<String> {
    store
        .log()
        .into_iter()
        .filter(|line| !line.starts_with("read "))
        .collect()
}

/// A rule check asking for a revision of the answer `short`.
fn no_short_answer() -> RuleCheck {
    RuleCheck::from_fn(Reads::new(), |ctx| {
        let text: String = ctx.answer().text().map(Text::as_str).collect();
        Ok(if text == "short" {
            Ruling::Revise(Text::new("Write more.")?)
        } else {
            Ruling::Pass
        })
    })
}

#[tokio::test]
async fn given_a_durable_call_when_it_runs_then_its_thread_holds_every_generation_and_message_of_the_critic()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(vec![text_step("first"), text_step("second")]);
    let critic_client = mock(vec![verdict(false, "add an example"), verdict(true, "")]);
    let graph = durable_duo(&generator_client, &critic_client, &store)
        .graph()
        .unwrap();
    let host = Host::new();

    let done = ended(
        call_on(&host, &graph, opening(&[said("i1", "explain")]), "", "r1").await,
        "validated",
    );

    assert_eq!(done.str(&key("thread")).unwrap(), nth_thread(1).as_str());
    let view = store.view(&nth_thread(1)).unwrap();
    let expected = [
        "user",
        "writer:first",
        "reviewer:add an example",
        "writer:second",
        "reviewer:Accepted.",
    ];
    assert_eq!(shape(&view.conversation), expected);
    assert_eq!(shape(done.transcript()), expected);
    assert!(
        view.messages
            .iter()
            .all(|message| message.status == MessageStatus::Completed)
    );
    // The critic works in no thread: only the messages the graph writes
    // reach it, each one text step of four events.
    for message in view
        .messages
        .iter()
        .filter(|m| m.author == Some(reviewer()))
    {
        let events = writes(&store)
            .iter()
            .filter(|line| **line == format!("event {}", message.id))
            .count();
        assert_eq!(events, 4, "{}", message.id);
    }
    let Some(acceptance) = view.conversation.entries().last() else {
        panic!("expected the acceptance");
    };
    let br_llm_messages::Entry::Turn(acceptance) = acceptance else {
        panic!("expected a turn");
    };
    assert_eq!(
        acceptance.items().len(),
        1,
        "one text step: {:?}",
        acceptance.items()
    );
    assert!(matches!(
        acceptance.state(),
        br_llm_messages::TurnState::Finished { stop_reason: StopReason::Other { ref reason } } if reason == "accepted"
    ));
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

#[tokio::test]
async fn given_the_same_script_when_the_generator_critic_runs_durable_and_in_memory_then_the_same_outputs_and_requests()
 {
    let script = || {
        (
            mock(vec![text_step("first"), text_step("second")]),
            mock(vec![verdict(false, "add an example"), verdict(true, "")]),
        )
    };
    let question = opening(&[said("i1", "explain")]);

    let (memory_generator, memory_critic) = script();
    let graph = duo(&memory_generator, &memory_critic).graph().unwrap();
    let in_memory = ended(call(&graph, question.clone()).await, "validated");

    let (durable_generator, durable_critic) = script();
    let store = Arc::new(TestStore::new());
    let graph = durable_duo(&durable_generator, &durable_critic, &store)
        .graph()
        .unwrap();
    let durable = ended(
        call_on(&Host::new(), &graph, question, "", "r1").await,
        "validated",
    );

    assert_eq!(
        in_memory.str(&key("answer")).unwrap(),
        durable.str(&key("answer")).unwrap()
    );
    assert_eq!(
        in_memory.int(&key("nb_generations")).unwrap(),
        durable.int(&key("nb_generations")).unwrap()
    );
    assert_eq!(shape(in_memory.transcript()), shape(durable.transcript()));
    for (memory, durable) in [
        (&memory_generator, &durable_generator),
        (&memory_critic, &durable_critic),
    ] {
        let sent = |client: &MockClient| -> Vec<_> {
            requests(client)
                .iter()
                .map(|request| request.messages().to_vec())
                .collect()
        };
        assert_eq!(sent(memory), sent(durable));
    }
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

#[tokio::test]
async fn given_a_durable_generator_critic_when_its_opening_or_its_thread_is_refused_then_opening_not_inputs_or_thread_not_found()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(Vec::new());
    let graph = durable_duo(&generator_client, &mock(Vec::new()), &store)
        .graph()
        .unwrap();
    let mut with_turn = opening(&[said("i1", "explain")]);
    with_turn
        .push_turn(Turn::new(
            TurnId::new("x1").unwrap(),
            Some(writer()),
            text_step("earlier"),
        ))
        .unwrap();

    let (_, error) = failed_in(
        call_on(&Host::new(), &graph, with_turn, "", "r1").await,
        "route",
    );
    assert!(matches!(error, GraphError::OpeningNotInputs), "{error}");

    let (_, error) = failed_in(
        call_on(
            &Host::new(),
            &graph,
            opening(&[said("i1", "explain")]),
            nth_thread(7).as_str(),
            "r1",
        )
        .await,
        "route",
    );
    assert!(
        matches!(error, GraphError::ThreadNotFound { .. }),
        "{error}"
    );
    assert_eq!(generator_client.calls(), 0);
}
