//! A durable ReAct agent resumed from its thread after a crash: a turn
//! found open is failed and repaired, never continued; a completed one
//! answers without a model call; an interrupted tool call is answered as
//! interrupted and not run again; a tool's updates are not recovered; the
//! opening is never stored twice; the same run finds its ids, a new run or
//! a new trigger gets new ones; a checkpoint enters again through `enter`.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::Reply;
use serde_json::json;

use crate::react::MessageStatus;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::*;
use crate::react::test_support::*;
use crate::run::{Outcome, control};
use crate::testkit::{INTERRUPTED, Op, TestStore};

fn unavailable() -> ClientError {
    ClientError::Unavailable { retry_after: None }
}

fn status(store: &TestStore, id: &str) -> MessageStatus {
    store
        .view(&nth_thread(1))
        .unwrap()
        .message(id)
        .unwrap()
        .status
}

fn count(store: &TestStore, line: &str) -> usize {
    store.log().iter().filter(|seen| *seen == line).count()
}

#[tokio::test]
async fn given_a_turn_left_open_by_a_crash_when_the_run_resumes_then_it_is_failed_repaired_and_the_call_goes_on_in_the_next_rank()
 {
    let store = Arc::new(TestStore::new());
    let writer = Arc::new(WriteFileTool::default());
    let client = mock(vec![tool_call_step("c1", "write_file", json!({}))]);
    client.push_reply(Reply::failure(unavailable()).build().unwrap());
    push(&client, text_step("written"));
    let graph = ReactAgent {
        tools: vec![writer.clone()],
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "write it")]);

    let crashed = failure(call_durable(&host, &graph, question.clone(), "", "r1").await);
    assert_eq!(failed_node(&crashed.error).0, "llm");
    assert_eq!(status(&store, "t2"), MessageStatus::Streaming);

    let done = finished(call_durable(&host, &graph, question, "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "written");
    assert_eq!(status(&store, "t2"), MessageStatus::Failed);
    assert_eq!(status(&store, "t3"), MessageStatus::Completed);
    assert_eq!(count(&store, "end t2 Failed"), 1);
    assert_eq!(count(&store, "open t2"), 1, "a turn is never opened again");
    assert_eq!(
        count(&store, "input i1"),
        1,
        "a restart never adds an input twice"
    );
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);
    assert_eq!(last_turn(&done).id().as_str(), "t3");
    assert_eq!(
        sent_results(&requests(&client)[2]),
        [("write_file".to_owned(), false, "written".to_owned())],
        "the model reads the result the crashed turn holds"
    );
}

#[tokio::test]
async fn given_a_tool_call_interrupted_by_a_crash_when_the_run_resumes_then_the_model_reads_it_interrupted_and_it_is_not_run_again()
 {
    let store = Arc::new(TestStore::new());
    let writer = Arc::new(WriteFileTool::default());
    let client = mock(vec![
        tool_call_step("c1", "write_file", json!({})),
        text_step("not sure it was written"),
    ]);
    let graph = ReactAgent {
        tools: vec![writer.clone()],
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "write it")]);
    store.fail(Op::PushResults, 1);

    let crashed = failure(call_durable(&host, &graph, question.clone(), "", "r1").await);
    let (node, error) = failed_node(&crashed.error);
    assert_eq!(node, "tools");
    assert_eq!(
        store_error(error),
        crate::testkit::StoreError::Injected(Op::PushResults)
    );
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1);

    let done = finished(call_durable(&host, &graph, question, "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "not sure it was written");
    assert_eq!(writer.calls.load(Ordering::SeqCst), 1, "not run again");
    assert_eq!(
        sent_results(&requests(&client)[1]),
        [("write_file".to_owned(), true, INTERRUPTED.to_owned())]
    );
}

#[tokio::test]
async fn given_a_turn_completed_before_a_crash_when_the_run_resumes_then_it_answers_from_the_thread_without_a_model_call()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![text_step("the answer")]);
    let graph = durable(&client, &store).graph().unwrap();
    let host = Host::new();
    let (sender, mut channel) = control();
    store.after(Op::EndTurn, 1, move || sender.cancel());

    let state = start(&graph, opening(&[said("i1", "hi")]), "");
    let stopped = run_in(&host, &graph, state, "r1", &mut channel).await;
    let Ok(Outcome::Cancelled { checkpoint }) = stopped.result else {
        panic!("expected the run cancelled as the turn completed");
    };
    assert_eq!(checkpoint.cursor.active, [nid("enter")]);
    assert_eq!(status(&store, "t2"), MessageStatus::Completed);
    assert_eq!(
        checkpoint.state.str(&key("thread")).unwrap(),
        nth_thread(1).as_str()
    );

    let done = finished(run_on(&host, &graph, checkpoint.state, "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "the answer");
    assert_eq!(thread_of(&done), nth_thread(1));
    assert_eq!(client.calls(), 1, "no model call on resume");
    assert_eq!(count(&store, "open t2"), 1);
}

#[tokio::test]
async fn given_a_crash_between_a_tool_result_written_and_its_updates_when_the_run_resumes_then_the_updates_are_not_recovered_and_the_tool_does_not_run_again()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "tally", json!({})),
        text_step("tallied once"),
    ]);
    let graph = ReactAgent {
        tools: vec![Arc::new(TallyTool)],
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let (sender, mut channel) = control();
    store.after(Op::PushResults, 1, move || sender.cancel());

    let state = start(&graph, opening(&[said("i1", "count")]), "");
    let stopped = run_in(&host, &graph, state, "r1", &mut channel).await;
    let Ok(Outcome::Cancelled { checkpoint }) = stopped.result else {
        panic!("expected the run cancelled once the result was written");
    };
    assert!(checkpoint.state.list(&key("log")).unwrap().is_empty());

    let done = finished(run_on(&host, &graph, checkpoint.state, "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "tallied once");
    assert!(
        done.list(&key("log")).unwrap().is_empty(),
        "the tool's update is not recovered, and the tool does not run again"
    );
    assert_eq!(
        sent_results(&requests(&client)[1]),
        [("tally".to_owned(), false, "tallied".to_owned())]
    );
    assert_eq!(status(&store, "t2"), MessageStatus::Failed);
}

#[tokio::test]
async fn given_a_crashed_call_when_a_new_run_or_a_new_trigger_comes_then_new_turn_ids() {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    client.push_reply(
        Reply::cut(text_step("half"), 1, unavailable())
            .build()
            .unwrap(),
    );
    push(&client, text_step("new run"));
    push(&client, text_step("new trigger"));
    let graph = durable(&client, &store).graph().unwrap();
    let host = Host::new();
    let thread = nth_thread(1);

    failure(call_durable(&host, &graph, opening(&[said("i1", "hi")]), "", "r1").await);
    assert_eq!(status(&store, "t2"), MessageStatus::Streaming);

    let new_run = finished(
        call_durable(
            &host,
            &graph,
            opening(&[said("i1", "hi")]),
            thread.as_str(),
            "r2",
        )
        .await,
    );
    let new_run_turn = last_turn(&new_run).id().as_str().to_owned();
    assert_ne!(new_run_turn, "t2");
    assert_eq!(
        status(&store, "t2"),
        MessageStatus::Streaming,
        "another run does not find the turns of r1"
    );

    let new_trigger = finished(
        call_durable(
            &host,
            &graph,
            opening(&[said("i2", "again")]),
            thread.as_str(),
            "r2",
        )
        .await,
    );
    assert_eq!(new_trigger.str(&key("answer")).unwrap(), "new trigger");
    let new_trigger_turn = last_turn(&new_trigger).id().as_str().to_owned();
    assert_ne!(new_trigger_turn, new_run_turn);
    assert_ne!(new_trigger_turn, "t2");
}

#[tokio::test]
async fn given_a_paused_durable_agent_when_resumed_from_its_checkpoint_then_it_omits_conversation_and_enters_again()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done"),
    ]);
    let graph = durable(&client, &store).graph().unwrap();
    let host = Host::new();
    let (sender, mut channel) = control();
    sender.pause();

    let state = start(&graph, opening(&[said("i1", "hi")]), "");
    let paused = run_in(&host, &graph, state, "r1", &mut channel).await;
    let Ok(Outcome::Paused { checkpoint }) = paused.result else {
        panic!("expected paused");
    };
    let keys: Vec<&str> = checkpoint
        .state
        .schema()
        .state
        .keys()
        .map(|key| key.as_str())
        .collect();
    assert_eq!(keys, ["answer", "log", "opening", "thread"]);
    assert_eq!(checkpoint.cursor.active, [nid("enter")]);
    assert!(checkpoint.pending.is_empty());
    assert_eq!(
        checkpoint.state.str(&key("thread")).unwrap(),
        nth_thread(1).as_str()
    );

    let done = finished(run_on(&host, &graph, checkpoint.state, "r1").await);
    assert_eq!(done.str(&key("answer")).unwrap(), "done");
    assert_eq!(count(&store, "input i1"), 1);
    assert_eq!(count(&store, "open t2"), 1);
}
