//! A durable ReAct agent entering its thread: the thread created under an
//! id from the id source or given, the opening stored once, the refusals of
//! `thread` and `opening`, and the same outputs as in memory.

use std::sync::Arc;

use br_llm_messages::{Turn, TurnId};

use crate::error::GraphError;
use crate::graph::IdRequest;
use crate::origin::{Origin, RunId};
use crate::react::MessageStatus;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::*;
use crate::react::test_support::*;
use crate::react::thread_store::ThreadStore;
use crate::testkit::TestStore;

/// `lines` with consecutive repeats collapsed.
fn collapsed(lines: Vec<String>) -> Vec<String> {
    let mut lines = lines;
    lines.dedup();
    lines
}

#[tokio::test]
async fn given_an_empty_thread_when_the_agent_answers_then_the_store_creates_its_thread_and_holds_the_whole_call()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![text_step("hello")]);
    let graph = durable(&client, &store).graph().unwrap();
    let recorder = Arc::new(StreamRecorder::default());
    let host = Host::observed_by(recorder.clone());

    let done = finished(call_durable(&host, &graph, opening(&[said("i1", "hi")]), "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "hello");
    let thread = thread_of(&done);
    assert_eq!(thread, nth_thread(1));
    let origin = Origin {
        run: RunId::new("r1"),
        occurrence: Default::default(),
    };
    assert_eq!(
        host.ids.requests()[0],
        IdRequest::new(origin, Some(said("i1", "hi").id().clone()), 0)
    );
    assert_eq!(
        collapsed(writes(&store)),
        [
            format!("create {thread}"),
            "input i1".to_owned(),
            "open t2".to_owned(),
            "event t2".to_owned(),
            "end t2 Completed".to_owned(),
        ]
    );
    let events = writes(&store)
        .iter()
        .filter(|line| *line == "event t2")
        .count();
    assert_eq!(events, recorder.events().len());
    assert!(
        recorder
            .events()
            .iter()
            .all(|(_, key, _)| key == "conversation")
    );

    let view = store.view(&thread).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Completed);
    assert_eq!(view.message("t2").unwrap().author, Some(author()));
    let turn: &Turn = view.conversation.turn(&TurnId::new("t2").unwrap()).unwrap();
    assert_eq!(last_turn(&done).id(), turn.id());
    assert_eq!(
        view.conversation.entries().len(),
        done.transcript().entries().len()
    );
}

#[tokio::test]
async fn given_a_thread_the_store_does_not_hold_when_the_agent_enters_then_thread_not_found_and_nothing_is_created()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![text_step("hello")]);
    let graph = durable(&client, &store).graph().unwrap();
    let given = nth_thread(7);

    let failure = failure(
        call_durable(
            &Host::new(),
            &graph,
            opening(&[said("i1", "hi")]),
            given.as_str(),
            "r1",
        )
        .await,
    );

    let (node, error) = failed_node(&failure.error);
    assert_eq!(node, "enter");
    assert!(matches!(error, GraphError::ThreadNotFound { thread } if *thread == given));
    assert!(writes(&store).is_empty());
    assert_eq!(client.calls(), 0);
}

#[tokio::test]
async fn given_a_thread_that_is_not_a_uuid_v7_when_the_agent_enters_then_thread_id() {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![text_step("hello")]);
    let graph = durable(&client, &store).graph().unwrap();

    let failure = failure(
        call_durable(
            &Host::new(),
            &graph,
            opening(&[said("i1", "hi")]),
            "thread-1",
            "r1",
        )
        .await,
    );

    let (node, error) = failed_node(&failure.error);
    assert_eq!(node, "enter");
    assert!(matches!(error, GraphError::ThreadId { value } if value == "thread-1"));
    assert!(store.log().is_empty());
}

#[tokio::test]
async fn given_an_opening_without_user_inputs_only_when_a_durable_agent_enters_then_opening_not_inputs()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    let graph = durable(&client, &store).graph().unwrap();
    let mut with_a_turn = opening(&[said("i1", "hi")]);
    with_a_turn
        .push_turn(Turn::new(
            TurnId::new("t0").unwrap(),
            Some(author()),
            text_step("earlier"),
        ))
        .unwrap();

    for opening in [opening(&[]), with_a_turn] {
        let failure = failure(call_durable(&Host::new(), &graph, opening, "", "r1").await);
        let (node, error) = failed_node(&failure.error);
        assert_eq!(node, "enter");
        assert!(matches!(error, GraphError::OpeningNotInputs));
    }
    assert!(store.log().is_empty());
}

#[tokio::test]
async fn given_a_trigger_the_caller_already_stored_when_the_agent_enters_then_only_the_inputs_the_thread_lacks_are_stored()
 {
    let store = Arc::new(TestStore::new());
    let thread = nth_thread(9);
    store.create(&thread, &Origin::default()).await.unwrap();
    store
        .store_input(&thread, &said("i2", "the question"))
        .await
        .unwrap();
    let client = mock(vec![text_step("the reply")]);
    let graph = durable(&client, &store).graph().unwrap();

    let opening = opening(&[said("i1", "some context"), said("i2", "the question")]);
    let done = finished(call_durable(&Host::new(), &graph, opening, thread.as_str(), "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "the reply");
    assert_eq!(thread_of(&done), thread);
    let inputs: Vec<String> = writes(&store)
        .into_iter()
        .filter(|line| line.starts_with("input "))
        .collect();
    assert_eq!(inputs, ["input i2", "input i1"]);
    let creates = writes(&store)
        .into_iter()
        .filter(|line| line.starts_with("create"))
        .count();
    assert_eq!(creates, 1, "only the caller's");
}

#[tokio::test]
async fn given_the_same_script_when_the_agent_runs_durable_and_in_memory_then_the_same_outputs_and_requests()
 {
    let script = || {
        vec![
            tool_call_step("c1", "echo", serde_json::json!({})),
            text_step("echo said echoed"),
        ]
    };
    let in_memory = mock(script());
    let graph = agent(&in_memory).graph().unwrap();
    let done = finished(call(&graph, opening(&[said("i1", "hi")])).await);

    let store = Arc::new(TestStore::new());
    let kept = mock(script());
    let durable = durable(&kept, &store).graph().unwrap();
    let kept_done = finished(
        call_durable(
            &Host::new(),
            &durable,
            opening(&[said("i1", "hi")]),
            "",
            "r1",
        )
        .await,
    );

    assert_eq!(
        done.str(&key("answer")).unwrap(),
        kept_done.str(&key("answer")).unwrap()
    );
    let messages = |client| -> Vec<_> {
        requests(client)
            .iter()
            .map(|request| request.messages().to_vec())
            .collect()
    };
    assert_eq!(messages(&in_memory), messages(&kept));
}
