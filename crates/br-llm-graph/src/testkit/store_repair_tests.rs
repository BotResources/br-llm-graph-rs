//! The test store repairs a failed or cancelled turn as a real store does:
//! every tool call without a result answered as interrupted, an unfinished
//! step kept when br-llm-messages can finish it, else dropped, so the thread
//! always renders.

use br_llm_messages::{Entry, StopReason, Text, ToolResultBlock, TurnItem};

use crate::react::{MessageStatus, ThreadStore, TurnEnd};
use crate::testkit::store_test_support::*;
use crate::testkit::{INTERRUPTED, INTERRUPTED_STOP};

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
