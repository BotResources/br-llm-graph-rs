use br_agentic_contract::{Completed, Opened, ThreadAccess, ThreadError};

use crate::common::*;

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
