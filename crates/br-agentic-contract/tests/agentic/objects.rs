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

#[tokio::test]
async fn given_a_pending_call_when_a_result_is_opened_then_a_stream_is_given_and_completing_adds_the_whole_result()
 {
    let world = World::new();
    let call = block_id();
    let turn = world.open_ai_turn_with_call(call).await;
    let entry = entry_id();
    let Opened::Stream(mut stream) = world
        .access
        .open_result(&world.thread, world.agent, world.ai(turn), entry, call)
        .await
        .unwrap()
    else {
        panic!("expected a stream");
    };
    stream.send("par".to_owned()).await;
    stream.send("tial".to_owned()).await;
    assert!(world.projected().await.entry(&entry).is_err());

    let completed = world
        .access
        .complete_result(
            &world.thread,
            world.agent,
            world.ai(turn),
            entry,
            tool_result(call, "search", "whole"),
        )
        .await
        .unwrap();
    stream.send("late".to_owned()).await;

    assert_eq!(completed, Completed::Added);
    assert_eq!(world.hub.result_fragments(entry), vec!["par", "tial"]);
    assert!(world.projected().await.entry(&entry).is_ok());
    assert!(!world.hub.is_open(entry));
}

#[tokio::test]
async fn given_a_call_that_is_not_pending_when_a_result_is_opened_then_the_opening_is_refused_by_state()
 {
    let world = World::new();
    let turn = world.open_ai_turn_with_call(block_id()).await;

    let error = world
        .access
        .open_result(
            &world.thread,
            world.agent,
            world.ai(turn),
            entry_id(),
            block_id(),
        )
        .await
        .err()
        .unwrap();

    assert!(matches!(error, ThreadError::Refused(_)));
}

#[tokio::test]
async fn given_a_result_for_a_call_that_is_not_pending_when_it_is_completed_then_the_content_check_refuses_it_with_a_reason()
 {
    let world = World::new();
    let turn = world.open_ai_turn_with_call(block_id()).await;
    let before = world.hub.conversation(&world.thread).unwrap();

    let error = world
        .access
        .complete_result(
            &world.thread,
            world.agent,
            world.ai(turn),
            entry_id(),
            tool_result(block_id(), "search", "x"),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ThreadError::Refused(reason) if !reason.as_str().is_empty()));
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
}

#[tokio::test]
async fn given_a_result_the_thread_holds_when_it_is_opened_then_the_held_result_is_answered_and_no_stream_is_given()
 {
    let world = World::new();
    let call = block_id();
    let turn = world.open_ai_turn_with_call(call).await;
    let entry = entry_id();
    world
        .access
        .complete_result(
            &world.thread,
            world.agent,
            world.ai(turn),
            entry,
            tool_result(call, "search", "kept"),
        )
        .await
        .unwrap();

    let opened = world
        .access
        .open_result(&world.thread, world.agent, world.ai(turn), entry, call)
        .await
        .unwrap();

    match opened {
        Opened::Held(results) => assert_eq!(results.id(), entry),
        Opened::Stream(_) => panic!("no stream is given for a held result"),
    }
}
