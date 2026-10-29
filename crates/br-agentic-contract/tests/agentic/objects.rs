use br_agentic_contract::{Completed, Opened, ThreadAccess, ThreadError, TurnTarget};
use br_llm_messages::{
    ClosureReason, DeltaOp, ResponseDelta, ResponseEnding, ResponseMetadata, TurnClosure,
};

use crate::common::*;

fn delta(turn: br_llm_messages::TurnId, entry: br_llm_messages::EntryId) -> ResponseDelta {
    ResponseDelta::new(
        turn,
        entry,
        DeltaOp::response_started(ResponseMetadata::new()),
    )
}

#[tokio::test]
async fn given_a_new_ai_turn_when_a_response_is_opened_then_a_stream_is_given_and_nothing_is_in_the_projection()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    let entry = entry_id();

    let opened = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry)
        .await
        .unwrap();

    assert!(matches!(opened, Opened::Stream(_)));
    assert!(world.hub.is_open(entry));
    assert!(world.projected().await.entry(&entry).is_err());
    assert!(world.projected().await.turn(&turn).is_err());
}

#[tokio::test]
async fn given_an_open_response_when_deltas_are_sent_then_the_implementer_takes_them_in_order() {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    let entry = entry_id();
    let Opened::Stream(mut stream) = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry)
        .await
        .unwrap()
    else {
        panic!("expected a stream");
    };

    stream.send(delta(turn, entry)).await;
    stream
        .send(ResponseDelta::new(
            turn,
            entry,
            DeltaOp::response_completed(ResponseEnding::finished()),
        ))
        .await;

    let sent = world.hub.response_deltas(entry);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].op().kind(), delta(turn, entry).op().kind());
}

#[tokio::test]
async fn given_an_open_response_when_it_is_completed_then_the_whole_response_is_added_the_stream_ends_and_later_sends_are_ignored()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    let entry = entry_id();
    let Opened::Stream(mut stream) = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry)
        .await
        .unwrap()
    else {
        panic!("expected a stream");
    };
    stream.send(delta(turn, entry)).await;

    let completed = world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry, "done"),
        )
        .await
        .unwrap();
    stream.send(delta(turn, entry)).await;

    assert_eq!(completed, Completed::Added);
    assert!(!world.hub.is_open(entry));
    assert!(world.projected().await.entry(&entry).is_ok());
    assert_eq!(world.hub.response_deltas(entry).len(), 1);
}

#[tokio::test]
async fn given_an_open_response_when_it_is_discarded_then_nothing_is_added_the_stream_ends_and_later_sends_are_ignored()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    let entry = entry_id();
    let Opened::Stream(mut stream) = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry)
        .await
        .unwrap()
    else {
        panic!("expected a stream");
    };
    stream.send(delta(turn, entry)).await;
    let before = world.hub.conversation(&world.thread).unwrap();

    world
        .access
        .discard_response(&world.thread, world.agent, entry)
        .await
        .unwrap();
    stream.send(delta(turn, entry)).await;

    assert!(!world.hub.is_open(entry));
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
    assert_eq!(world.hub.response_deltas(entry).len(), 1);
}

#[tokio::test]
async fn given_an_open_response_when_it_is_opened_again_then_the_earlier_stream_ends_with_its_partial_content_dropped_and_a_new_stream_begins()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    let entry = entry_id();
    let Opened::Stream(mut first) = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry)
        .await
        .unwrap()
    else {
        panic!("expected a stream");
    };
    first.send(delta(turn, entry)).await;

    let Opened::Stream(mut second) = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry)
        .await
        .unwrap()
    else {
        panic!("expected a stream");
    };
    first.send(delta(turn, entry)).await;
    assert!(world.hub.response_deltas(entry).is_empty());
    second.send(delta(turn, entry)).await;

    assert_eq!(world.hub.response_deltas(entry).len(), 1);
    assert!(world.hub.is_open(entry));
}

#[tokio::test]
async fn given_an_object_that_is_not_open_when_it_is_discarded_then_it_is_accepted_and_nothing_changes()
 {
    let world = World::new();
    world.start_with_human("hi").await;
    let before = world.hub.conversation(&world.thread).unwrap();

    world
        .access
        .discard_response(&world.thread, world.agent, entry_id())
        .await
        .unwrap();
    world
        .access
        .discard_result(&world.thread, world.agent, entry_id())
        .await
        .unwrap();

    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
}

#[tokio::test]
async fn given_a_response_never_opened_when_it_is_completed_then_it_is_accepted() {
    let world = World::new();
    world.start_with_human("hi").await;
    let entry = entry_id();

    let completed = world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn_id()),
            response_text(entry, "direct"),
        )
        .await
        .unwrap();

    assert_eq!(completed, Completed::Added);
}

#[tokio::test]
async fn given_a_closed_ai_turn_when_a_response_is_opened_then_the_opening_is_refused_by_state() {
    let world = World::new();
    world.start_with_human("hi").await;
    let turn = turn_id();
    world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry_id(), "a"),
        )
        .await
        .unwrap();
    world
        .access
        .close_turn(
            &world.thread,
            world.agent,
            turn,
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await
        .unwrap();

    let error = world
        .access
        .open_response(&world.thread, world.agent, world.ai(turn), entry_id())
        .await
        .err()
        .unwrap();

    assert!(matches!(error, ThreadError::Refused(_)));
}

#[tokio::test]
async fn given_an_input_turn_target_that_does_not_exist_when_a_response_is_opened_then_it_is_refused()
 {
    let world = World::new();
    world.start_with_human("hi").await;

    let error = world
        .access
        .open_response(
            &world.thread,
            world.agent,
            TurnTarget::Input(turn_id()),
            entry_id(),
        )
        .await
        .err()
        .unwrap();

    assert!(matches!(error, ThreadError::Refused(_)));
}
