//! A durable ReAct agent and the replies it cannot use: with a structured
//! output mode a step reaches the thread once checked, so the store sees no
//! event of a refused step; in the plain mode a reply refused after some of
//! its events reached the thread fails the open turn and is asked for again
//! in the turn of the next rank, as a resumed run would.

use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::{Chunking, MockClient, Reply, derive_events};
use br_llm_messages::{AssistantBlock, Conversation, Entry, Step, StopReason, Turn, TurnItem};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::MessageStatus;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::{finished, found_shape, last_turn};
use crate::react::react_durable_support::*;
use crate::react::test_support::*;
use crate::testkit::TestStore;

/// The content of each step of `turn`, in order.
fn steps_of(turn: &Turn) -> Vec<Vec<AssistantBlock>> {
    turn.items()
        .iter()
        .filter_map(|item| match item {
            TurnItem::Step(step) => Some(step.content().to_vec()),
            TurnItem::ToolResults(_) => None,
        })
        .collect()
}

/// Each entry of `conversation` by its id, with the content of its steps
/// for a turn.
fn entries_of(conversation: &Conversation) -> Vec<(String, Vec<Vec<AssistantBlock>>)> {
    conversation
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::Turn(turn) => (turn.id().as_str().to_owned(), steps_of(turn)),
            Entry::UserInput(input) => (input.id().as_str().to_owned(), Vec::new()),
        })
        .collect()
}

/// The number of stream events `step` is served as.
fn events_of(step: &Step) -> usize {
    derive_events(step, &Chunking::none())
        .unwrap()
        .events()
        .len()
}

/// The durable agent on `client` in `store`, with `retries`.
fn retrying(client: &MockClient, store: &Arc<TestStore>, retries: usize) -> ReactAgent {
    ReactAgent {
        reply_retries: Some(Count::Fixed(retries)),
        ..durable(client, store)
    }
}

#[tokio::test]
async fn given_a_structured_durable_agent_when_a_reply_is_asked_again_then_the_store_sees_no_event_of_the_refused_step()
 {
    let store = Arc::new(TestStore::new());
    let fitting = structured_step(json!({ "answer": "the moon" }));
    let client = mock(vec![
        structured_step(json!({ "answer": 7, "detail": "unasked" })),
        fitting.clone(),
    ]);
    let graph = ReactAgent {
        output_mode: found_shape(),
        ..retrying(&client, &store, 1)
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

    assert_eq!(
        done.str(&key("answer")).unwrap(),
        r#"{"answer":"the moon"}"#
    );
    let mut expected = vec!["open t2".to_owned()];
    expected.extend(std::iter::repeat_n(
        "event t2".to_owned(),
        events_of(&fitting),
    ));
    expected.extend(["finish t2".to_owned(), "end t2 Completed".to_owned()]);
    assert_eq!(store_lines(&store, "t2"), expected);
    let view = store.view(&nth_thread(1)).unwrap();
    let held = view.conversation.turn(&named_turn("t2")).unwrap();
    assert_eq!(held.items().len(), 1, "only the checked step");
    assert_eq!(steps_of(held), steps_of(&last_turn(&done)));
}

#[tokio::test]
async fn given_a_structured_durable_agent_with_no_retry_when_the_reply_does_not_fit_then_it_fails_and_its_turn_holds_nothing()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![structured_step(json!({ "answer": 7 }))]);
    let graph = ReactAgent {
        output_mode: found_shape(),
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();

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

    let (node, error) = failed_node(&failure.error);
    assert_eq!(node, "llm");
    assert!(matches!(error, GraphError::Structured { .. }));
    assert_eq!(store_lines(&store, "t2"), ["open t2"]);
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Streaming);
    assert!(view.conversation.turn(&named_turn("t2")).is_none());
}

#[tokio::test]
async fn given_a_plain_durable_agent_when_the_client_refuses_a_reply_after_some_events_then_the_turn_fails_and_the_retry_is_made_in_the_next_rank()
 {
    let store = Arc::new(TestStore::new());
    let refused = ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    };
    let client = mock(Vec::new());
    client.push_reply(
        Reply::cut(text_step("the mo"), 2, refused.clone())
            .build()
            .unwrap(),
    );
    push(&client, text_step("the moon"));
    let graph = retrying(&client, &store, 1).graph().unwrap();
    let host = Host::new();

    let done =
        finished(call_durable(&host, &graph, opening(&[said("i1", "where")]), "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "the moon");
    assert_eq!(
        store_lines(&store, "t2"),
        ["open t2", "event t2", "event t2", "end t2 Failed"]
    );
    assert_eq!(
        store_lines(&store, "t3")
            .into_iter()
            .filter(|line| !line.starts_with("event"))
            .collect::<Vec<_>>(),
        ["open t3", "finish t3", "end t3 Completed"]
    );
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(view.message("t3").unwrap().status, MessageStatus::Completed);
    assert_eq!(
        entries_of(done.transcript()),
        entries_of(&view.conversation),
        "the state holds what the thread holds"
    );
    assert_eq!(last_turn(&done).id(), &named_turn("t3"));
    let sent = requests(&client);
    let reminder = crate::react::generator_critic_view_support::user_message(&format!(
        "Your reply could not be used: {refused}. Reply again."
    ));
    assert_eq!(sent[1].messages().last(), Some(&reminder));

    let again = finished(
        call_durable(
            &host,
            &graph,
            opening(&[said("i1", "where")]),
            thread_of(&done).as_str(),
            "r1",
        )
        .await,
    );
    assert_eq!(again.str(&key("answer")).unwrap(), "the moon");
    assert_eq!(client.calls(), 2, "a resumed run finds the call answered");
}

#[tokio::test]
async fn given_a_plain_durable_agent_when_the_client_refuses_a_reply_before_any_event_then_the_retry_is_made_in_the_same_turn()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    client.push_reply(
        Reply::failure(ClientError::MalformedResponse {
            detail: "no reply".to_owned(),
        })
        .build()
        .unwrap(),
    );
    push(&client, text_step("the moon"));
    let graph = retrying(&client, &store, 1).graph().unwrap();

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
    let t2 = store_lines(&store, "t2");
    assert_eq!(t2.first().map(String::as_str), Some("open t2"));
    assert_eq!(t2.iter().filter(|line| *line == "open t2").count(), 1);
    assert_eq!(t2.last().map(String::as_str), Some("end t2 Completed"));
    assert!(store_lines(&store, "t3").is_empty());
}

#[tokio::test]
async fn given_a_plain_durable_agent_with_no_retry_when_the_client_refuses_after_some_events_then_model_and_the_turn_stays_open_until_resumed()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    client.push_reply(
        Reply::cut(
            text_step("the mo"),
            2,
            ClientError::ReplyUnusable {
                stop_reason: StopReason::MaxTokens,
            },
        )
        .build()
        .unwrap(),
    );
    let graph = durable(&client, &store).graph().unwrap();
    let host = Host::new();

    let failure =
        failure(call_durable(&host, &graph, opening(&[said("i1", "where")]), "", "r1").await);

    let (_, error) = failed_node(&failure.error);
    assert!(matches!(
        error,
        GraphError::Model(ClientError::ReplyUnusable { .. })
    ));
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Streaming);

    push(&client, text_step("the moon"));
    let resumed = finished(
        call_durable(
            &host,
            &graph,
            opening(&[said("i1", "where")]),
            nth_thread(1).as_str(),
            "r1",
        )
        .await,
    );
    assert_eq!(resumed.str(&key("answer")).unwrap(), "the moon");
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(view.message("t3").unwrap().status, MessageStatus::Completed);
}
