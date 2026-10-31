use std::sync::atomic::Ordering;

use br_agentic_contract::ThreadAccess;
use br_agentic_contract::memory::Call;
use br_agentic_contract::{PortError, Session, SessionError};
use br_llm_messages::{ClosureReason, InputTurn, TurnClosure, TurnEntry};

use crate::common::*;
use crate::flaky::Flaky;

fn reads(world: &World) -> usize {
    world
        .record
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Read { .. }))
        .count()
}

fn handed_entries(world: &World) -> usize {
    world
        .record
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::AddEntry { .. } | Call::CompleteResponse { .. }))
        .count()
}

#[tokio::test]
async fn given_a_session_when_it_starts_then_the_thread_is_read_once_and_nothing_is_read_during_the_run()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (turn, call) = world.open_turn_with_call(&mut session).await;
    session
        .complete_result(&turn, tool_results(entry_id(), call, "lookup", "found"))
        .await
        .unwrap();
    session
        .complete_response(&turn, response_text(entry_id(), "more"))
        .await
        .unwrap();
    session
        .close_turn(
            &turn,
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await
        .unwrap();

    assert_eq!(reads(&world), 1);
}

#[tokio::test]
async fn given_a_conversation_so_far_when_the_session_starts_then_the_instance_works_on_its_own_copy_of_it()
 {
    let world = World::new();
    world
        .access
        .add_input_turn(
            &world.thread,
            InputTurn::new(turn_id(), vec![human("trigger").into_entry()]).unwrap(),
        )
        .await
        .unwrap();

    let session = world.session().await;

    assert_eq!(session.conversation(), &world.access.conversation());
}

#[tokio::test]
async fn given_a_thread_access_that_cannot_give_the_conversation_when_the_session_starts_then_it_is_a_fault_and_the_read_is_not_retried()
 {
    let world = World::new();
    let flaky = Flaky::new(world.access.clone());
    flaky.fail_reads.store(true, Ordering::SeqCst);

    let started = Session::start(&flaky, world.thread).await;

    assert!(matches!(started, Err(PortError::Fault(_))));
    assert_eq!(flaky.failed_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn given_an_addition_when_the_session_stores_it_then_it_is_in_the_copy_and_handed_to_the_thread_access()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let entry = entry_id();

    session
        .add_entry(&turn, TurnEntry::from(response_text(entry, "hi")))
        .await
        .unwrap();

    assert!(session.holds(entry));
    assert!(world.access.conversation().entry(&entry).is_ok());
}

#[tokio::test]
async fn given_an_addition_that_the_rules_refuse_when_the_session_stores_it_then_it_is_not_handed_to_the_thread_access_and_the_refusal_is_a_fault()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let entry = entry_id();
    session
        .add_entry(&turn, TurnEntry::from(response_text(entry, "once")))
        .await
        .unwrap();
    let handed = handed_entries(&world);

    let error = session
        .add_entry(&turn, TurnEntry::from(response_text(entry, "twice")))
        .await
        .unwrap_err();

    assert!(matches!(error, SessionError::Rules(_)));
    assert_eq!(handed_entries(&world), handed);
}

#[tokio::test]
async fn given_a_turn_that_exists_with_another_agent_or_author_when_an_entry_is_added_then_it_is_refused_and_not_handed_over()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    session
        .add_entry(&turn, TurnEntry::from(response_text(entry_id(), "mine")))
        .await
        .unwrap();
    let mut other_author = turn.clone();
    other_author.author = author("someone else");
    let mut other_agent = turn.clone();
    other_agent.agent = agent_id();
    let handed = handed_entries(&world);

    for named in [other_author, other_agent] {
        let error = session
            .add_entry(&named, TurnEntry::from(response_text(entry_id(), "x")))
            .await
            .unwrap_err();
        assert!(matches!(error, SessionError::WrongTurn(_)));
    }

    assert_eq!(handed_entries(&world), handed);
}

#[tokio::test]
async fn given_a_closure_for_a_turn_that_does_not_exist_when_it_is_stored_then_it_is_refused() {
    let world = World::new();
    let mut session = world.session().await;

    let error = session
        .close_turn(
            &world.ai(turn_id()),
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, SessionError::WrongTurn(_)));
}

#[tokio::test]
async fn given_an_open_response_when_the_session_opens_it_then_the_object_is_not_in_the_copy_and_is_tracked()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    let entry = entry_id();

    let _stream = session.open_response(&turn, entry).await.unwrap();

    assert!(!session.holds(entry));
    assert_eq!(session.open_objects(), 1);
}

mod independence;
mod streaming;
