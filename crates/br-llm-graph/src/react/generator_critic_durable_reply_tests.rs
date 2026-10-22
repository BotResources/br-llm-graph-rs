//! A durable generator-critic's generator and the replies it cannot use:
//! with a structured output mode the thread holds only the checked step; in
//! the plain mode a reply refused after some of its events reached the
//! thread fails its turn, and the retry, in the turn of the next rank, is no
//! generation.

use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::Reply;
use br_llm_messages::TurnId;
use serde_json::json;

use crate::graph::Count;
use crate::react::MessageStatus;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::{call_on, durable_duo, writes};
use crate::react::generator_critic_support::*;
use crate::react::react_durable_support::{Host, nth_thread, opening, said};
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;
use crate::testkit::TestStore;

#[tokio::test]
async fn given_a_durable_structured_generator_when_its_reply_is_asked_again_then_the_thread_holds_only_the_checked_step()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(vec![
        structured_step(json!({ "text": false })),
        structured_step(json!({ "text": "fine" })),
    ]);
    let graph = GeneratorCritic {
        generator: drafting(&generator_client, Some(Count::Fixed(1))),
        ..durable_duo(
            &generator_client,
            &mock(vec![verdict(true, "Good.")]),
            &store,
        )
    }
    .graph()
    .unwrap();

    let done = ended(
        call_on(
            &Host::new(),
            &graph,
            opening(&[said("i1", "explain")]),
            "",
            "r1",
        )
        .await,
        "validated",
    );

    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    let view = store.view(&nth_thread(1)).unwrap();
    let generation = view.conversation.turn(&TurnId::new("t2").unwrap()).unwrap();
    assert_eq!(generation.items().len(), 1);
    let finished = store.finished(&nth_thread(1), &TurnId::new("t2").unwrap());
    assert_eq!(finished.len(), 1);
    assert_eq!(
        finished[0].structured().next(),
        Some(&json!({ "text": "fine" }))
    );
}

#[tokio::test]
async fn given_a_durable_plain_generator_when_its_reply_is_refused_after_some_events_then_the_retry_in_the_next_rank_is_no_generation()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(Vec::new());
    generator_client.push_reply(
        Reply::cut(
            text_step("a dra"),
            2,
            ClientError::MalformedResponse {
                detail: "a broken stream".to_owned(),
            },
        )
        .build()
        .unwrap(),
    );
    push(&generator_client, text_step("a draft"));
    let graph = GeneratorCritic {
        generator: GeneratorSeat {
            reply_retries: Some(Count::Fixed(1)),
            ..generator(&generator_client)
        },
        ..durable_duo(
            &generator_client,
            &mock(vec![verdict(true, "Good.")]),
            &store,
        )
    }
    .graph()
    .unwrap();

    let done = ended(
        call_on(
            &Host::new(),
            &graph,
            opening(&[said("i1", "explain")]),
            "",
            "r1",
        )
        .await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "a draft");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    assert!(writes(&store).contains(&"end t2 Failed".to_owned()));
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(view.message("t3").unwrap().status, MessageStatus::Completed);
}
