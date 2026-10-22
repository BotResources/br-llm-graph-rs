//! The test store behaves as a real store: each write repeated safely by
//! its own rule, steps folded from their events, and faults injected on
//! demand.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use br_llm_messages::Author;

use crate::react::{MessageStatus, ThreadStore, TurnEnd};
use crate::testkit::store_test_support::*;
use crate::testkit::{Op, StoreError, TestStore};

#[tokio::test]
async fn given_an_absent_thread_when_read_then_none() {
    let store = TestStore::new();
    assert!(store.read(&thread()).await.unwrap().is_none());
}

#[tokio::test]
async fn given_a_thread_created_when_created_again_then_done_for_its_origin_and_refused_for_another()
 {
    let store = TestStore::new();
    store.create(&thread(), &origin("")).await.unwrap();
    store.create(&thread(), &origin("")).await.unwrap();
    let view = store.read(&thread()).await.unwrap().unwrap();
    assert!(view.messages.is_empty());
    let error = store.create(&thread(), &origin("other")).await.unwrap_err();
    assert!(matches!(stored(error), StoreError::Conflict(_)));
}

#[tokio::test]
async fn given_an_input_stored_when_stored_again_then_done_once_and_another_content_is_refused() {
    let store = with_open_turn().await;
    store
        .store_input(&thread(), &input("i1", "hi"))
        .await
        .unwrap();
    let view = store.view(&thread()).unwrap();
    assert_eq!(view.conversation.entries().len(), 1);
    assert_eq!(view.message("i1").unwrap().status, MessageStatus::Completed);
    let error = store
        .store_input(&thread(), &input("i1", "other"))
        .await
        .unwrap_err();
    assert!(matches!(stored(error), StoreError::Conflict(_)));
    let error = store
        .store_input(&thread(), &input("t1", "hi"))
        .await
        .unwrap_err();
    assert!(matches!(stored(error), StoreError::Conflict(_)));
}

#[tokio::test]
async fn given_an_input_for_an_absent_thread_when_stored_then_absent() {
    let store = TestStore::new();
    let error = store
        .store_input(&thread(), &input("i1", "hi"))
        .await
        .unwrap_err();
    assert_eq!(stored(error), StoreError::Absent(thread()));
}

#[tokio::test]
async fn given_a_turn_opened_when_opened_again_then_done_for_its_author_and_refused_for_another() {
    let store = with_open_turn().await;
    store
        .open_turn(&thread(), &turn("t1"), &agent())
        .await
        .unwrap();
    let other = Author::new("other", "other").unwrap();
    let error = store
        .open_turn(&thread(), &turn("t1"), &other)
        .await
        .unwrap_err();
    assert!(matches!(stored(error), StoreError::Conflict(_)));
    let error = store
        .open_turn(&thread(), &turn("i1"), &agent())
        .await
        .unwrap_err();
    assert!(matches!(stored(error), StoreError::Conflict(_)));
    assert_eq!(store.view(&thread()).unwrap().messages.len(), 2);
}

#[tokio::test]
async fn given_the_events_of_a_step_when_pushed_then_the_step_joins_the_open_turn_at_its_finish() {
    let store = with_open_turn().await;
    let events = text_events("hello");
    push_all(&store, "t1", &events[..3]).await;
    let view = store.view(&thread()).unwrap();
    assert!(view.conversation.turn(&turn("t1")).is_none());
    assert_eq!(view.message("t1").unwrap().status, MessageStatus::Streaming);
    push_all(&store, "t1", &events[3..]).await;
    let view = store.view(&thread()).unwrap();
    let held = view.conversation.turn(&turn("t1")).unwrap();
    assert_eq!(held.items().len(), 1);
    assert_eq!(held.author(), Some(&agent()));
}

#[tokio::test]
async fn given_a_turn_completed_when_ended_again_then_done_and_another_end_is_refused() {
    let store = with_open_turn().await;
    push_all(&store, "t1", &text_events("hello")).await;
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Completed)
        .await
        .unwrap();
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Completed)
        .await
        .unwrap();
    assert_eq!(status(&store, "t1"), MessageStatus::Completed);
    let error = store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Failed)
        .await
        .unwrap_err();
    assert!(matches!(stored(error), StoreError::Conflict(_)));
    let error = store
        .push_event(&thread(), &turn("t1"), &text_events("more")[0])
        .await
        .unwrap_err();
    assert_eq!(stored(error), StoreError::NotOpen(turn("t1")));
}

#[tokio::test]
async fn given_a_tool_results_item_held_at_its_index_when_pushed_again_then_done_once() {
    let store = with_open_turn().await;
    push_all(&store, "t1", &call_events("c1", "{}")).await;
    store
        .push_results(&thread(), &turn("t1"), 1, &[result("c1")])
        .await
        .unwrap();
    store
        .push_results(&thread(), &turn("t1"), 1, &[result("c1")])
        .await
        .unwrap();
    let view = store.view(&thread()).unwrap();
    assert_eq!(
        view.conversation.turn(&turn("t1")).unwrap().items().len(),
        2
    );
    let error = store
        .push_results(&thread(), &turn("t1"), 3, &[result("c1")])
        .await
        .unwrap_err();
    assert!(matches!(stored(error), StoreError::Refused(_)));
}

#[tokio::test]
async fn given_a_fault_injected_on_a_call_when_it_comes_then_it_fails_and_writes_nothing() {
    let store = with_open_turn().await;
    store.fail(Op::PushEvent, 2);
    let events = text_events("hello");
    store
        .push_event(&thread(), &turn("t1"), &events[0])
        .await
        .unwrap();
    let error = store
        .push_event(&thread(), &turn("t1"), &events[1])
        .await
        .unwrap_err();
    assert_eq!(stored(error), StoreError::Injected(Op::PushEvent));
    assert_eq!(store.calls(Op::PushEvent), 2);
    assert_eq!(
        store.log().last().map(String::as_str),
        Some("event t1 refused")
    );
}

#[tokio::test]
async fn given_an_action_planned_after_a_call_when_it_succeeds_then_the_action_runs_once() {
    let store = with_open_turn().await;
    let ran = Arc::new(AtomicBool::new(false));
    let flag = ran.clone();
    store.after(Op::EndTurn, 1, move || flag.store(true, Ordering::SeqCst));
    push_all(&store, "t1", &text_events("hello")).await;
    assert!(!ran.load(Ordering::SeqCst));
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Completed)
        .await
        .unwrap();
    assert!(ran.load(Ordering::SeqCst));
}

/// The step `body` folds into, with the model `model`.
fn answered(body: &str, model: &str) -> br_llm_messages::Step {
    br_llm_messages::Step::new(
        vec![br_llm_messages::AssistantBlock::Text {
            text: br_llm_messages::Text::new(body).unwrap(),
        }],
        br_llm_messages::StopReason::EndTurn,
        None,
        Some(br_llm_messages::ModelId::new(model).unwrap()),
    )
    .unwrap()
}

#[tokio::test]
async fn given_a_step_folded_from_its_events_when_it_is_finished_then_the_store_keeps_it_with_its_model()
 {
    let store = with_open_turn().await;
    for event in text_events("hello") {
        store
            .push_event(&thread(), &turn("t1"), &event)
            .await
            .unwrap();
    }
    store
        .finish_step(&thread(), &turn("t1"), &answered("hello", "m1"))
        .await
        .unwrap();
    let finished = store.finished(&thread(), &turn("t1"));
    assert_eq!(finished, [answered("hello", "m1")]);
    assert_eq!(store.log().last().map(String::as_str), Some("finish t1"));
}

#[tokio::test]
async fn given_a_step_the_store_has_not_folded_when_it_is_finished_then_refused() {
    let store = with_open_turn().await;
    let error = store
        .finish_step(&thread(), &turn("t1"), &answered("hello", "m1"))
        .await
        .unwrap_err();
    assert!(matches!(stored(error), StoreError::Refused(_)));
    let events = text_events("hello");
    for event in &events[..2] {
        store
            .push_event(&thread(), &turn("t1"), event)
            .await
            .unwrap();
    }
    let error = store
        .finish_step(&thread(), &turn("t1"), &answered("hello", "m1"))
        .await
        .unwrap_err();
    assert!(
        matches!(stored(error), StoreError::Refused(_)),
        "a step under way"
    );
    for event in &events[2..] {
        store
            .push_event(&thread(), &turn("t1"), event)
            .await
            .unwrap();
    }
    let error = store
        .finish_step(&thread(), &turn("t1"), &answered("goodbye", "m1"))
        .await
        .unwrap_err();
    assert!(
        matches!(stored(error), StoreError::Refused(_)),
        "another step"
    );
    assert!(store.finished(&thread(), &turn("t1")).is_empty());
}
