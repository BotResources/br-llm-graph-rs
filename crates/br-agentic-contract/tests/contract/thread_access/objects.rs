use super::*;

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
