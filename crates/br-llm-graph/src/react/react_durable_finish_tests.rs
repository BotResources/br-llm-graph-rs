//! The finished step a durable agent gives its store: once per step a
//! client returned, after its last event, with the model that answered and
//! its usage; a store that does not keep it, and one that fails on it.

use std::sync::Arc;

use br_llm_messages::{
    AssistantBlock, Author, ModelId, Step, StopReason, StreamEvent, Text, ToolResult, TurnId,
    Usage, UserInput,
};
use serde_json::json;

use crate::error::GraphError;
use crate::origin::Origin;
use crate::react::MessageStatus;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::finished;
use crate::react::react_durable_support::*;
use crate::react::test_support::*;
use crate::react::thread_store::{ThreadFuture, ThreadStore, ThreadView, TurnEnd};
use crate::testkit::TestStore;
use crate::value::ThreadId;

/// A step answering `body`, with a usage.
fn counted(body: &str) -> Step {
    Step::new(
        vec![AssistantBlock::Text {
            text: Text::new(body).unwrap(),
        }],
        StopReason::EndTurn,
        Some(Usage {
            input_tokens: 12,
            output_tokens: 3,
            cache_read_tokens: None,
            cache_write_tokens: None,
            thinking_tokens: None,
        }),
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn given_a_durable_agent_when_a_step_ends_then_the_store_is_given_it_with_its_model_and_usage()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        counted("echoed"),
    ]);
    let graph = durable(&client, &store).graph().unwrap();

    let done = finished(
        call_durable(
            &Host::new(),
            &graph,
            opening(&[said("i1", "echo")]),
            "",
            "r1",
        )
        .await,
    );

    let finished_steps = store.finished(&thread_of(&done), &named_turn("t2"));
    assert_eq!(
        finished_steps.len(),
        2,
        "one per step, the tool round's included"
    );
    let model = ModelId::new("mock-model").unwrap();
    assert!(
        finished_steps
            .iter()
            .all(|step| step.model() == Some(&model))
    );
    assert_eq!(finished_steps[1].usage(), counted("echoed").usage());
    assert_eq!(finished_steps[1].content(), counted("echoed").content());
    let lines = store_lines(&store, "t2");
    let finish = lines.iter().rposition(|line| line == "finish t2").unwrap();
    let last_event = lines.iter().rposition(|line| line == "event t2").unwrap();
    assert!(last_event < finish, "given after the step's last event");
    assert_eq!(lines.last().map(String::as_str), Some("end t2 Completed"));
}

/// A store that keeps nothing of a finished step: it delegates every
/// operation to a `TestStore` and leaves `finish_step` to its default.
struct Forgetful(TestStore);

impl ThreadStore for Forgetful {
    fn read<'a>(&'a self, thread: &'a ThreadId) -> ThreadFuture<'a, Option<ThreadView>> {
        self.0.read(thread)
    }

    fn create<'a>(&'a self, thread: &'a ThreadId, origin: &'a Origin) -> ThreadFuture<'a, ()> {
        self.0.create(thread, origin)
    }

    fn store_input<'a>(
        &'a self,
        thread: &'a ThreadId,
        input: &'a UserInput,
    ) -> ThreadFuture<'a, ()> {
        self.0.store_input(thread, input)
    }

    fn open_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        author: &'a Author,
    ) -> ThreadFuture<'a, ()> {
        self.0.open_turn(thread, turn, author)
    }

    fn push_event<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        event: &'a StreamEvent,
    ) -> ThreadFuture<'a, ()> {
        self.0.push_event(thread, turn, event)
    }

    fn push_results<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        index: usize,
        results: &'a [ToolResult],
    ) -> ThreadFuture<'a, ()> {
        self.0.push_results(thread, turn, index, results)
    }

    fn end_turn<'a>(
        &'a self,
        thread: &'a ThreadId,
        turn: &'a TurnId,
        end: TurnEnd,
    ) -> ThreadFuture<'a, ()> {
        self.0.end_turn(thread, turn, end)
    }
}

#[tokio::test]
async fn given_a_store_that_does_not_implement_finish_step_when_the_agent_answers_then_the_default_does_nothing()
 {
    let store = Arc::new(Forgetful(TestStore::new()));
    let client = mock(vec![text_step("the moon")]);
    let graph = ReactAgent {
        thread_store: Some(store.clone()),
        ..durable(&client, &Arc::new(TestStore::new()))
    }
    .graph()
    .unwrap();

    let done = finished(
        call_durable(
            &Host::new(),
            &graph,
            opening(&[said("i1", "where")]),
            "",
            "r1",
        )
        .await,
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "the moon");
    assert!(store.0.log().iter().all(|line| !line.starts_with("finish")));
    assert!(
        store
            .0
            .finished(&thread_of(&done), &named_turn("t2"))
            .is_empty()
    );
}

#[tokio::test]
async fn given_the_store_failing_to_take_a_finished_step_when_the_agent_answers_then_thread_and_the_turn_is_not_completed()
 {
    let store = Arc::new(TestStore::new());
    store.fail(crate::testkit::Op::FinishStep, 1);
    let client = mock(vec![text_step("the moon")]);
    let graph = durable(&client, &store).graph().unwrap();

    let failure = failure(
        call_durable(
            &Host::new(),
            &graph,
            opening(&[said("i1", "where")]),
            "",
            "r1",
        )
        .await,
    );

    let (_, error) = failed_node(&failure.error);
    assert!(matches!(error, GraphError::Thread { .. }));
    assert_eq!(
        store
            .view(&nth_thread(1))
            .unwrap()
            .message("t2")
            .unwrap()
            .status,
        MessageStatus::Streaming
    );
}
