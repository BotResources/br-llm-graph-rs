//! The test store behaves as a real store: each write repeated safely by
//! its own rule, steps folded from their events, failed and cancelled turns
//! repaired so the thread always renders, and faults injected on demand.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use br_llm_messages::{
    Author, BlockKind, Entry, InputId, Perspective, StopReason, StreamEvent, Text, ToolCallId,
    ToolName, ToolResult, ToolResultBlock, TurnId, TurnItem, UserBlock, UserInput, UserSource,
    render,
};

use crate::origin::{OccurrenceKey, Origin, RunId};
use crate::react::{MessageStatus, ThreadStore, TurnEnd};
use crate::testkit::{INTERRUPTED, INTERRUPTED_STOP, Op, StoreError, TestStore};
use crate::value::ThreadId;

fn thread() -> ThreadId {
    ThreadId::new("01900000-0000-7000-8000-000000000001").unwrap()
}

fn origin(occurrence: &str) -> Origin {
    Origin {
        run: RunId::new("r1"),
        occurrence: OccurrenceKey::parse(occurrence).unwrap(),
    }
}

fn agent() -> Author {
    Author::new("agent", "agent").unwrap()
}

fn turn(id: &str) -> TurnId {
    TurnId::new(id).unwrap()
}

fn input(id: &str, body: &str) -> UserInput {
    UserInput::new(
        InputId::new(id).unwrap(),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

fn text_events(body: &str) -> Vec<StreamEvent> {
    vec![
        StreamEvent::BlockStart {
            index: 0,
            kind: BlockKind::Text,
        },
        StreamEvent::TextDelta {
            index: 0,
            text: body.to_owned(),
        },
        StreamEvent::BlockEnd { index: 0 },
        StreamEvent::Finish {
            stop_reason: StopReason::EndTurn,
            usage: None,
        },
    ]
}

fn call_events(id: &str, arguments: &str) -> Vec<StreamEvent> {
    vec![
        StreamEvent::ToolCallStart {
            index: 0,
            id: ToolCallId::new(id).unwrap(),
            name: ToolName::new("echo").unwrap(),
        },
        StreamEvent::ToolCallArgumentsDelta {
            index: 0,
            json_fragment: arguments.to_owned(),
        },
        StreamEvent::BlockEnd { index: 0 },
        StreamEvent::Finish {
            stop_reason: StopReason::AwaitingToolResults,
            usage: None,
        },
    ]
}

fn result(id: &str) -> ToolResult {
    ToolResult::new(
        ToolCallId::new(id).unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    )
}

/// A store holding the thread with one user input and the open turn `t1`.
async fn with_open_turn() -> TestStore {
    let store = TestStore::new();
    store.create(&thread(), &origin("")).await.unwrap();
    store
        .store_input(&thread(), &input("i1", "hi"))
        .await
        .unwrap();
    store
        .open_turn(&thread(), &turn("t1"), &agent())
        .await
        .unwrap();
    store
}

async fn push_all(store: &TestStore, id: &str, events: &[StreamEvent]) {
    for event in events {
        store.push_event(&thread(), &turn(id), event).await.unwrap();
    }
}

fn status(store: &TestStore, id: &str) -> MessageStatus {
    store.view(&thread()).unwrap().message(id).unwrap().status
}

fn stored(error: crate::react::ThreadError) -> StoreError {
    error.downcast_ref::<StoreError>().unwrap().clone()
}

fn renders(store: &TestStore) {
    let view = store.view(&thread()).unwrap();
    render(&view.conversation, &Perspective::new(agent())).unwrap();
}

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
async fn given_a_turn_awaiting_tool_results_when_failed_then_each_call_is_answered_as_interrupted()
{
    let store = with_open_turn().await;
    push_all(&store, "t1", &call_events("c1", "{}")).await;
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Failed)
        .await
        .unwrap();
    assert_eq!(status(&store, "t1"), MessageStatus::Failed);
    let view = store.view(&thread()).unwrap();
    let held = view.conversation.turn(&turn("t1")).unwrap();
    let Some(TurnItem::ToolResults(results)) = held.items().last() else {
        panic!("expected the interrupted results");
    };
    let answered = &results.results()[0];
    assert!(answered.is_error);
    assert_eq!(
        answered.content,
        vec![ToolResultBlock::text(Text::new(INTERRUPTED).unwrap())]
    );
    renders(&store);
}

#[tokio::test]
async fn given_a_text_step_cut_mid_stream_when_cancelled_then_it_is_finished_and_kept() {
    let store = with_open_turn().await;
    push_all(&store, "t1", &text_events("half a reply")[..2]).await;
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Cancelled)
        .await
        .unwrap();
    assert_eq!(status(&store, "t1"), MessageStatus::Cancelled);
    let view = store.view(&thread()).unwrap();
    let held = view.conversation.turn(&turn("t1")).unwrap();
    let Some(TurnItem::Step(step)) = held.items().first() else {
        panic!("expected the kept step");
    };
    assert_eq!(
        step.stop_reason(),
        &StopReason::Other {
            reason: INTERRUPTED_STOP.to_owned()
        }
    );
    renders(&store);
}

#[tokio::test]
async fn given_a_tool_call_cut_in_its_arguments_when_failed_then_the_step_is_dropped() {
    let store = with_open_turn().await;
    push_all(&store, "t1", &call_events("c1", "{\"text\": \"ha")[..2]).await;
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Failed)
        .await
        .unwrap();
    let view = store.view(&thread()).unwrap();
    assert!(view.conversation.turn(&turn("t1")).is_none());
    assert_eq!(view.message("t1").unwrap().status, MessageStatus::Failed);
    assert!(matches!(view.conversation.entries(), [Entry::UserInput(_)]));
}

#[tokio::test]
async fn given_a_whole_tool_call_without_its_finish_when_failed_then_it_is_kept_and_answered_as_interrupted()
 {
    let store = with_open_turn().await;
    push_all(&store, "t1", &text_events("first")).await;
    // Not a step the turn takes: a finished turn awaits no step, so the cut
    // second step is dropped and the turn keeps its first.
    push_all(&store, "t1", &call_events("c1", "{}")[..3]).await;
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Failed)
        .await
        .unwrap();
    let view = store.view(&thread()).unwrap();
    assert_eq!(
        view.conversation.turn(&turn("t1")).unwrap().items().len(),
        1
    );

    let store = with_open_turn().await;
    push_all(&store, "t1", &call_events("c1", "{}")[..3]).await;
    store
        .end_turn(&thread(), &turn("t1"), TurnEnd::Failed)
        .await
        .unwrap();
    let view = store.view(&thread()).unwrap();
    let held = view.conversation.turn(&turn("t1")).unwrap();
    assert_eq!(held.items().len(), 2);
    renders(&store);
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
