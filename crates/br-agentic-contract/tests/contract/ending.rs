use std::sync::Arc;
use std::sync::atomic::Ordering;

use br_agentic_contract::memory::{Call, MemoryWorkspace};
use br_agentic_contract::{FileSystemMode, Session, cancelled, failed, result, save_files};
use br_graph_contract::{Event, Input, Instance, State};

use crate::common::*;
use crate::flaky::Flaky;

async fn open_two(
    world: &World,
    session: &mut Session<'_>,
) -> (br_llm_messages::EntryId, br_llm_messages::EntryId) {
    let (turn, _call) = world.open_turn_with_call(session).await;
    let (response, result_entry) = (entry_id(), entry_id());
    let _a = session.open_response(&turn, response).await.unwrap();
    let _b = session.open_result(&turn, result_entry).await.unwrap();
    (response, result_entry)
}

fn an_input() -> Input<Doc> {
    Input::new(Arc::new(State::create(Doc { n: 1 }).unwrap()))
}

#[tokio::test]
async fn given_open_objects_when_the_instance_ends_with_result_then_they_are_discarded_the_input_end_is_closed_and_the_event_is_the_result()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (response, result_entry) = open_two(&world, &mut session).await;
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();

    let event = result::<Doc>(&reader, &mut session, 5).await;

    assert!(matches!(event, Event::Result(5)));
    assert!(!world.access.is_open(response) && !world.access.is_open(result_entry));
    assert!(input_end.give(an_input()).is_err());
}

#[tokio::test]
async fn given_open_objects_when_the_instance_ends_with_failed_then_they_are_discarded_the_input_end_is_closed_and_the_event_is_failed()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (response, result_entry) = open_two(&world, &mut session).await;
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();

    let event = failed(&reader, &mut session, "tool fault").await;

    assert!(matches!(event, Event::Failed(cause) if cause.to_string() == "tool fault"));
    assert!(!world.access.is_open(response) && !world.access.is_open(result_entry));
    assert!(input_end.give(an_input()).is_err());
}

#[tokio::test]
async fn given_open_objects_when_the_instance_is_cancelled_then_they_are_discarded_nothing_stored_is_taken_back_the_input_end_is_closed_and_cancelled_is_sent()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (response, result_entry) = open_two(&world, &mut session).await;
    let stored = world.access.conversation();
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();

    let event = cancelled(&reader, &mut session).await;

    assert!(matches!(event, Event::Cancelled));
    assert!(!world.access.is_open(response) && !world.access.is_open(result_entry));
    assert_eq!(world.access.conversation(), stored);
    assert!(input_end.give(an_input()).is_err());
}

#[tokio::test]
async fn given_open_objects_and_a_discard_that_fails_when_the_instance_is_cancelled_then_the_error_is_ignored_and_cancelled_is_sent()
 {
    let world = World::new();
    let flaky = Flaky::new(world.access.clone());
    let mut session = Session::start(&flaky, world.thread).await.unwrap();
    open_two(&world, &mut session).await;
    flaky.fail_discards.store(true, Ordering::SeqCst);
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();

    let event = cancelled(&reader, &mut session).await;

    assert!(matches!(event, Event::Cancelled));
    assert_eq!(flaky.failed_calls.load(Ordering::SeqCst), 2);
    assert!(input_end.give(an_input()).is_err());
}

#[tokio::test]
async fn given_open_objects_and_a_discard_that_fails_when_the_instance_ends_with_result_then_the_error_is_ignored_the_input_end_is_closed_and_the_event_is_the_result()
 {
    let world = World::new();
    let flaky = Flaky::new(world.access.clone());
    let mut session = Session::start(&flaky, world.thread).await.unwrap();
    open_two(&world, &mut session).await;
    flaky.fail_discards.store(true, Ordering::SeqCst);
    let (instance, reader, _sender) = Instance::<Doc>::channel(instance_id());
    let (input_end, _event_end) = instance.into_parts();

    let event = result::<Doc>(&reader, &mut session, 5).await;

    assert!(matches!(event, Event::Result(5)));
    assert_eq!(flaky.failed_calls.load(Ordering::SeqCst), 2);
    assert!(input_end.give(an_input()).is_err());
}

#[tokio::test]
async fn given_a_discard_that_fails_when_the_instance_ends_with_failed_then_the_error_is_ignored_and_failed_is_sent()
 {
    let world = World::new();
    let flaky = Flaky::new(world.access.clone());
    let mut session = Session::start(&flaky, world.thread).await.unwrap();
    open_two(&world, &mut session).await;
    flaky.fail_discards.store(true, Ordering::SeqCst);
    let (_instance, reader, _sender) = Instance::<Doc>::channel(instance_id());

    let event = failed(&reader, &mut session, "boom").await;

    assert!(matches!(event, Event::Failed(cause) if cause.to_string() == "boom"));
}

#[tokio::test]
async fn given_the_start_of_a_file_system_when_the_instance_fails_or_is_cancelled_then_no_files_are_saved()
 {
    let world = World::new();
    let workspace = MemoryWorkspace::new(&world.record, Vec::new());
    let files = FileSystemMode::Always
        .start(&workspace, &fields())
        .await
        .unwrap();
    assert!(files.is_some());
    let mut session = world.session().await;
    let (_i, reader, _s) = Instance::<Doc>::channel(instance_id());

    failed(&reader, &mut session, "boom").await;
    cancelled(&reader, &mut session).await;

    assert!(
        !world
            .record
            .calls()
            .iter()
            .any(|call| matches!(call, Call::WorkspaceSave { .. }))
    );
    save_files(&workspace, &fields(), files).await.unwrap();
}
