//! A durable generator-critic: its conversation is its thread, every
//! generation and message of the critic written there; it gives the same
//! outputs and requests as the same call in memory; its opening and its
//! thread are refused as a durable agent's are (resumptions:
//! `generator_critic_resume_tests.rs`).

use std::sync::Arc;

use br_llm_clients_mock::MockClient;
use br_llm_messages::{StopReason, Turn, TurnId};

use crate::error::GraphError;
use crate::react::MessageStatus;
use crate::react::generator_critic_durable_support::*;
use crate::react::generator_critic_support::*;
use crate::react::react_durable_support::{Host, nth_thread, opening, said};
use crate::react::test_support::*;
use crate::testkit::TestStore;

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
