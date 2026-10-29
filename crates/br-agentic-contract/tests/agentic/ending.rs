use br_agentic_contract::{Opened, Session, ThreadAccess, cancelled, failed, result};
use br_graph_contract::{Event, Instance};

use crate::common::*;
use crate::flaky::Flaky;

async fn open_two(
    world: &World,
    session: &mut Session<'_>,
) -> (br_llm_messages::EntryId, br_llm_messages::EntryId) {
    let call = block_id();
    let turn = world.open_ai_turn_with_call(call).await;
    let response = entry_id();
    let result_entry = entry_id();
    let new_turn = turn_id();
    assert!(matches!(
        session
            .open_response(world.ai(new_turn), response)
            .await
            .unwrap(),
        Opened::Stream(_)
    ));
    assert!(matches!(
        session
            .open_result(world.ai(turn), result_entry, call)
            .await
            .unwrap(),
        Opened::Stream(_)
    ));
    (response, result_entry)
}

#[tokio::test]
async fn given_open_objects_when_the_session_discards_all_then_the_streams_end_and_nothing_is_added()
 {
    let world = World::new();
    let mut session = Session::new(&world.access, world.thread, world.agent);
    let (response, result_entry) = open_two(&world, &mut session).await;
    assert_eq!(session.open_objects(), 2);
    let before = world.hub.conversation(&world.thread).unwrap();

    let errors = session.discard_all().await;

    assert!(errors.is_empty());
    assert_eq!(session.open_objects(), 0);
    assert!(!world.hub.is_open(response) && !world.hub.is_open(result_entry));
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
}

#[tokio::test]
async fn given_an_object_the_thread_holds_when_the_session_opens_it_then_no_object_is_tracked_as_open()
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
    let mut session = Session::new(&world.access, world.thread, world.agent);

    let opened = session.open_response(world.ai(turn), entry).await.unwrap();

    assert!(matches!(opened, Opened::Held(_)));
    assert_eq!(session.open_objects(), 0);
}

#[tokio::test]
async fn given_an_open_response_when_the_session_completes_or_discards_it_then_it_is_no_longer_tracked()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let mut session = Session::new(&world.access, world.thread, world.agent);
    let (turn, kept, dropped) = (turn_id(), entry_id(), entry_id());
    session.open_response(world.ai(turn), kept).await.unwrap();
    session
        .open_response(world.ai(turn), dropped)
        .await
        .unwrap();

    session
        .complete_response(world.ai(turn), response_text(kept, "whole"))
        .await
        .unwrap();
    session.discard_response(dropped).await.unwrap();

    assert_eq!(session.open_objects(), 0);
    assert!(world.projected().await.entry(&kept).is_ok());
    assert!(world.projected().await.entry(&dropped).is_err());
}

#[tokio::test]
async fn given_a_failed_model_call_when_the_instance_retries_then_the_new_response_has_a_new_stream_and_new_identities()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let mut session = Session::new(&world.access, world.thread, world.agent);
    let (turn, first, second) = (turn_id(), entry_id(), entry_id());
    session.open_response(world.ai(turn), first).await.unwrap();
    session.discard_response(first).await.unwrap();

    session.open_response(world.ai(turn), second).await.unwrap();
    session
        .complete_response(world.ai(turn), response_text(second, "retry"))
        .await
        .unwrap();

    assert!(world.projected().await.entry(&first).is_err());
    assert!(world.projected().await.entry(&second).is_ok());
}

#[tokio::test]
async fn given_open_objects_when_the_instance_ends_with_failed_then_they_are_discarded_the_input_end_is_closed_and_the_event_is_failed()
 {
    let world = World::new();
    let mut session = Session::new(&world.access, world.thread, world.agent);
    let (response, result_entry) = open_two(&world, &mut session).await;
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();

    let event = failed(&reader, &mut session, "tool fault").await;

    assert!(matches!(event, Event::Failed(cause) if cause.to_string() == "tool fault"));
    assert!(!world.hub.is_open(response) && !world.hub.is_open(result_entry));
    let input = br_graph_contract::Input::new(std::sync::Arc::new(
        br_graph_contract::State::create(Doc { n: 1 }).unwrap(),
    ));
    assert!(input_end.give(input).is_err());
}

#[tokio::test]
async fn given_open_objects_and_a_discard_that_fails_when_the_instance_is_cancelled_then_the_error_is_ignored_and_cancelled_is_sent()
 {
    let world = World::new();
    let flaky = Flaky {
        inner: world.access.clone(),
        fail_adds: false,
        fail_discards: true,
    };
    let mut session = Session::new(&flaky, world.thread, world.agent);
    open_two(&world, &mut session).await;
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();
    let before = world.hub.conversation(&world.thread).unwrap();

    let event = cancelled(&reader, &mut session).await;

    assert!(matches!(event, Event::Cancelled));
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
    let input = br_graph_contract::Input::new(std::sync::Arc::new(
        br_graph_contract::State::create(Doc { n: 1 }).unwrap(),
    ));
    assert!(input_end.give(input).is_err());
}

#[tokio::test]
async fn given_the_end_of_an_instance_when_the_terminal_event_is_built_then_the_input_end_is_closed_first()
 {
    let world = World::new();
    let mut session = Session::new(&world.access, world.thread, world.agent);
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();

    let event = result::<Doc>(&reader, &mut session, 5).await;

    assert!(matches!(event, Event::Result(5)));
    let input = br_graph_contract::Input::new(std::sync::Arc::new(
        br_graph_contract::State::create(Doc { n: 1 }).unwrap(),
    ));
    assert!(input_end.give(input).is_err());
}
