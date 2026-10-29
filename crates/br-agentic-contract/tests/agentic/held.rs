use br_agentic_contract::{Completed, Opened, ThreadAccess};

use crate::common::*;

#[tokio::test]
async fn given_a_response_the_thread_holds_when_it_is_discarded_then_it_is_accepted_and_the_thread_still_holds_it()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let entry = entry_id();
    world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn_id()),
            response_text(entry, "kept"),
        )
        .await
        .unwrap();

    world
        .access
        .discard_response(&world.thread, world.agent, entry)
        .await
        .unwrap();

    assert!(world.projected().await.entry(&entry).is_ok());
}

#[tokio::test]
async fn given_a_response_the_thread_holds_when_it_is_opened_then_the_held_response_is_answered_and_no_stream_is_given()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    let entry = entry_id();
    world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry, "kept"),
        )
        .await
        .unwrap();

    let opened = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry)
        .await
        .unwrap();

    match opened {
        Opened::Held(response) => assert_eq!(response.id(), entry),
        Opened::Stream(_) => panic!("no stream is given for a held response"),
    }
    assert!(!world.hub.is_open(entry));
}

#[tokio::test]
async fn given_a_response_the_thread_holds_when_another_is_completed_with_the_same_identity_then_the_held_one_is_answered_and_nothing_is_added()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    let entry = entry_id();
    world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry, "first"),
        )
        .await
        .unwrap();
    let before = world.hub.conversation(&world.thread).unwrap();

    let completed = world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry, "second"),
        )
        .await
        .unwrap();

    match completed {
        Completed::Held(held) => assert_eq!(held, response_text_of(&before, entry)),
        Completed::Added => panic!("the thread holds the response already"),
    }
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
}

fn response_text_of(
    conversation: &br_llm_messages::Conversation,
    entry: br_llm_messages::EntryId,
) -> br_llm_messages::ModelResponse {
    match conversation.entry(&entry).unwrap() {
        br_llm_messages::EntryRef::Turn(br_llm_messages::TurnEntry::ModelResponse(response)) => {
            response.clone()
        }
        other => panic!("expected a response, got {other:?}"),
    }
}
