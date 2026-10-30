use br_agentic_contract::memory::Call;
use br_agentic_contract::{PortError, ThreadAccess};
use br_llm_messages::{
    DeltaOp, InputTurn, ResponseDelta, ResponseMetadata, Turn, TurnClosure, TurnEntry,
};

use crate::common::*;

fn input(text: &str) -> InputTurn {
    InputTurn::new(turn_id(), vec![human(text).into_entry()]).unwrap()
}

fn kept(world: &World, entry: br_llm_messages::EntryId) -> bool {
    world.access.conversation().entry(&entry).is_ok()
}

#[tokio::test]
async fn given_an_input_turn_when_it_is_added_then_the_read_gives_it_with_all_its_entries_in_order()
{
    let world = World::new();
    let first = human("one");
    let second = notice("two");
    let turn = InputTurn::new(
        turn_id(),
        vec![first.clone().into_entry(), second.clone().into_entry()],
    )
    .unwrap();

    world
        .access
        .add_input_turn(&world.thread, turn)
        .await
        .unwrap();

    let read = world.access.read(&world.thread).await.unwrap();
    let Turn::InputTurn(held) = &read.turns()[0] else {
        panic!("an input turn")
    };
    assert_eq!(held.entries(), [first.into_entry(), second.into_entry()]);
}

#[tokio::test]
async fn given_the_first_entry_of_a_new_ai_turn_when_it_is_added_then_the_turn_is_created_with_its_agent_and_author()
 {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();

    world
        .access
        .add_entry(
            &world.thread,
            turn.clone(),
            TurnEntry::from(response_text(entry, "hi")),
        )
        .await
        .unwrap();

    let read = world.access.read(&world.thread).await.unwrap();
    let Turn::AITurn(ai) = &read.turns()[0] else {
        panic!("an AI turn")
    };
    assert_eq!(
        (ai.id(), ai.agent(), ai.author()),
        (turn.id, turn.agent, &turn.author)
    );
    assert!(kept(&world, entry));
}

#[tokio::test]
async fn given_an_ai_turn_when_a_closure_is_added_then_the_turn_is_closed() {
    let world = World::new();
    let turn = world.ai(turn_id());
    world
        .access
        .add_entry(
            &world.thread,
            turn.clone(),
            TurnEntry::from(response_text(entry_id(), "hi")),
        )
        .await
        .unwrap();

    world
        .access
        .close_turn(
            &world.thread,
            turn.clone(),
            TurnClosure::new(entry_id(), br_llm_messages::ClosureReason::completed()),
        )
        .await
        .unwrap();

    let read = world.access.read(&world.thread).await.unwrap();
    assert!(read.turn(&turn.id).unwrap().as_ai().unwrap().is_closed());
}

#[tokio::test]
async fn given_an_open_response_when_the_test_reads_then_the_open_object_is_not_in_the_conversation()
 {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();

    let _stream = world
        .access
        .open_response(&world.thread, turn, entry)
        .await
        .unwrap();

    assert!(world.access.is_open(entry));
    assert!(
        world
            .access
            .read(&world.thread)
            .await
            .unwrap()
            .turns()
            .is_empty()
    );
}

fn delta() -> ResponseDelta {
    ResponseDelta::new(
        turn_id(),
        entry_id(),
        DeltaOp::response_started(ResponseMetadata::new()),
    )
}

fn deltas_recorded(world: &World) -> usize {
    world
        .record
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::ResponseDelta { .. }))
        .count()
}

#[tokio::test]
async fn given_an_open_response_when_it_is_completed_then_the_whole_response_is_in_the_conversation()
 {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let _stream = world
        .access
        .open_response(&world.thread, turn.clone(), entry)
        .await
        .unwrap();

    world
        .access
        .complete_response(&world.thread, turn, response_text(entry, "whole"))
        .await
        .unwrap();

    assert!(kept(&world, entry));
    assert!(!world.access.is_open(entry));
}

#[tokio::test]
async fn given_a_stream_that_has_ended_when_a_delta_is_sent_to_it_then_it_is_ignored() {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let mut stream = world
        .access
        .open_response(&world.thread, turn.clone(), entry)
        .await
        .unwrap();
    stream.send(delta()).await;
    world
        .access
        .complete_response(&world.thread, turn, response_text(entry, "whole"))
        .await
        .unwrap();

    stream.send(delta()).await;

    assert_eq!(deltas_recorded(&world), 1);
}

#[tokio::test]
async fn given_a_stream_when_its_object_is_opened_again_then_a_delta_sent_to_the_earlier_stream_is_ignored()
 {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let mut first = world
        .access
        .open_response(&world.thread, turn.clone(), entry)
        .await
        .unwrap();
    let mut second = world
        .access
        .open_response(&world.thread, turn, entry)
        .await
        .unwrap();

    first.send(delta()).await;
    second.send(delta()).await;

    assert_eq!(deltas_recorded(&world), 1);
}

#[tokio::test]
async fn given_an_open_result_when_a_fragment_is_sent_after_the_discard_then_it_is_ignored() {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let mut stream = world
        .access
        .open_result(&world.thread, turn, entry)
        .await
        .unwrap();
    stream.send("kept".to_owned()).await;
    world
        .access
        .discard_result(&world.thread, entry)
        .await
        .unwrap();

    stream.send("late".to_owned()).await;

    let fragments = world
        .record
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::ResultFragment { .. }))
        .count();
    assert_eq!(fragments, 1);
}

#[tokio::test]
async fn given_an_open_response_when_it_is_discarded_then_nothing_is_added_and_the_stream_ends() {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let _stream = world
        .access
        .open_response(&world.thread, turn, entry)
        .await
        .unwrap();

    world
        .access
        .discard_response(&world.thread, entry)
        .await
        .unwrap();

    assert!(!world.access.is_open(entry));
    assert!(
        world
            .access
            .read(&world.thread)
            .await
            .unwrap()
            .turns()
            .is_empty()
    );
}

#[tokio::test]
async fn given_an_object_that_is_not_open_when_it_is_discarded_then_the_discard_is_accepted_and_changes_nothing()
 {
    let world = World::new();

    world
        .access
        .discard_response(&world.thread, entry_id())
        .await
        .unwrap();
    world
        .access
        .discard_result(&world.thread, entry_id())
        .await
        .unwrap();

    assert!(
        world
            .access
            .read(&world.thread)
            .await
            .unwrap()
            .turns()
            .is_empty()
    );
}

#[tokio::test]
async fn given_an_object_that_is_already_open_when_it_is_opened_again_then_the_earlier_stream_ends_and_a_new_stream_begins()
 {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();
    let _first = world
        .access
        .open_response(&world.thread, turn.clone(), entry)
        .await
        .unwrap();

    let _second = world
        .access
        .open_response(&world.thread, turn, entry)
        .await
        .unwrap();

    assert!(world.access.is_open(entry));
}

#[tokio::test]
async fn given_an_object_that_was_not_opened_when_it_is_completed_then_it_is_added() {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();

    world
        .access
        .complete_response(&world.thread, turn, response_text(entry, "no stream"))
        .await
        .unwrap();

    assert!(kept(&world, entry));
}

#[tokio::test]
async fn given_an_addition_that_the_rules_of_br_llm_messages_refuse_when_it_is_received_then_the_answer_is_a_fault_and_the_conversation_is_unchanged()
 {
    let world = World::new();
    let turn = world.ai(turn_id());
    let entry = entry_id();
    world
        .access
        .complete_response(&world.thread, turn.clone(), response_text(entry, "once"))
        .await
        .unwrap();
    let before = world.access.conversation();

    let error = world
        .access
        .add_entry(
            &world.thread,
            turn,
            TurnEntry::from(response_text(entry, "twice")),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, PortError::Fault(_)));
    assert_eq!(world.access.conversation(), before);
}

#[tokio::test]
async fn given_the_in_memory_thread_access_when_it_receives_calls_then_it_keeps_them_in_order_in_the_record_and_only_the_completed_additions_in_the_conversation()
 {
    let world = World::new();
    let turn = world.ai(turn_id());
    let streamed = entry_id();
    let dropped = entry_id();
    world
        .access
        .add_input_turn(&world.thread, input("hello"))
        .await
        .unwrap();
    let _ = world
        .access
        .open_response(&world.thread, turn.clone(), dropped)
        .await
        .unwrap();
    world
        .access
        .discard_response(&world.thread, dropped)
        .await
        .unwrap();
    world
        .access
        .complete_response(&world.thread, turn, response_text(streamed, "x"))
        .await
        .unwrap();

    let names: Vec<&str> = world
        .record
        .calls()
        .iter()
        .map(|call| match call {
            Call::AddInputTurn { .. } => "input",
            Call::OpenResponse { .. } => "open",
            Call::DiscardResponse { .. } => "discard",
            Call::CompleteResponse { .. } => "complete",
            _ => "other",
        })
        .collect();
    assert_eq!(names, ["input", "open", "discard", "complete"]);
    assert!(kept(&world, streamed));
    assert!(!kept(&world, dropped));
}
