//! What the tests of the test store share: a thread, its open turn, the
//! events of a step and readings of the store.

use br_llm_messages::{
    Author, BlockKind, InputId, Perspective, StopReason, StreamEvent, Text, ToolCallId, ToolName,
    ToolResult, ToolResultBlock, TurnId, UserBlock, UserInput, UserSource, render,
};

use crate::origin::{OccurrenceKey, Origin, RunId};
use crate::react::{MessageStatus, ThreadStore};
use crate::testkit::{StoreError, TestStore};
use crate::value::ThreadId;

pub(crate) fn thread() -> ThreadId {
    ThreadId::new("01900000-0000-7000-8000-000000000001").unwrap()
}

pub(crate) fn origin(occurrence: &str) -> Origin {
    Origin {
        run: RunId::new("r1"),
        occurrence: OccurrenceKey::parse(occurrence).unwrap(),
    }
}

pub(crate) fn agent() -> Author {
    Author::new("agent", "agent").unwrap()
}

pub(crate) fn turn(id: &str) -> TurnId {
    TurnId::new(id).unwrap()
}

pub(crate) fn input(id: &str, body: &str) -> UserInput {
    UserInput::new(
        InputId::new(id).unwrap(),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

pub(crate) fn text_events(body: &str) -> Vec<StreamEvent> {
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

pub(crate) fn call_events(id: &str, arguments: &str) -> Vec<StreamEvent> {
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

pub(crate) fn result(id: &str) -> ToolResult {
    ToolResult::new(
        ToolCallId::new(id).unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    )
}

/// A store holding the thread with one user input and the open turn `t1`.
pub(crate) async fn with_open_turn() -> TestStore {
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

pub(crate) async fn push_all(store: &TestStore, id: &str, events: &[StreamEvent]) {
    for event in events {
        store.push_event(&thread(), &turn(id), event).await.unwrap();
    }
}

pub(crate) fn status(store: &TestStore, id: &str) -> MessageStatus {
    store.view(&thread()).unwrap().message(id).unwrap().status
}

pub(crate) fn stored(error: crate::react::ThreadError) -> StoreError {
    error.downcast_ref::<StoreError>().unwrap().clone()
}

pub(crate) fn renders(store: &TestStore) {
    let view = store.view(&thread()).unwrap();
    render(&view.conversation, &Perspective::new(agent())).unwrap();
}
