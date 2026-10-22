//! A durable ReAct agent whose turn was failed under a reply the client
//! refused: on a later round of the turn, the rounds it answered stay in the
//! failed turn and the reply is asked for again in the turn of the next
//! rank; a crash after the turn was failed and before the next one was
//! opened is resumed as any run is, from the thread.

use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::Reply;
use br_llm_messages::{StopReason, TurnItem};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::Count;
use crate::react::MessageStatus;
use crate::react::generator_critic_view_support::user_message;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::{finished, last_turn};
use crate::react::react_durable_support::*;
use crate::react::test_support::*;
use crate::testkit::{Op, TestStore};

/// The error a refused reply carries.
fn refused() -> ClientError {
    ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    }
}

#[tokio::test]
async fn given_a_reply_refused_on_a_later_round_when_a_retry_is_left_then_the_failed_turn_keeps_its_rounds_and_the_retry_opens_the_next_rank()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    client.push_reply(
        Reply::cut(text_step("echo sa"), 2, refused())
            .build()
            .unwrap(),
    );
    push(&client, text_step("echo said echoed"));
    let graph = ReactAgent {
        reply_retries: Some(Count::Fixed(1)),
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();

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

    assert_eq!(done.str(&key("answer")).unwrap(), "echo said echoed");
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(view.message("t3").unwrap().status, MessageStatus::Completed);
    let failed = view.conversation.turn(&named_turn("t2")).unwrap();
    let rounds = failed
        .items()
        .iter()
        .filter(|item| matches!(item, TurnItem::ToolResults(_)))
        .count();
    assert_eq!(rounds, 1, "the round it answered stays in the failed turn");
    assert_eq!(last_turn(&done).id(), &named_turn("t3"));
    assert_eq!(last_turn(&done).items().len(), 1, "the retry is a new turn");
    let sent = requests(&client);
    assert_eq!(sent.len(), 3);
    assert_eq!(
        sent[2].messages().last(),
        Some(&user_message(&format!(
            "Your reply could not be used: {}. Reply again.",
            refused()
        )))
    );
    assert_eq!(
        sent_results(&sent[2]),
        [("echo".to_owned(), false, "echoed".to_owned())],
        "the retry reads the round the failed turn holds"
    );
}

#[tokio::test]
async fn given_a_crash_after_the_turn_is_failed_and_before_the_next_is_opened_when_the_call_runs_again_then_it_goes_on_in_the_next_rank()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    client.push_reply(
        Reply::cut(text_step("the mo"), 2, refused())
            .build()
            .unwrap(),
    );
    let graph = ReactAgent {
        reply_retries: Some(Count::Fixed(1)),
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    store.fail(Op::OpenTurn, 2);

    let crashed =
        failure(call_durable(&host, &graph, opening(&[said("i1", "where")]), "", "r1").await);

    let (node, error) = failed_node(&crashed.error);
    assert_eq!(node, "llm");
    assert!(matches!(error, GraphError::Thread { .. }));
    assert_eq!(client.calls(), 1, "the retry was never sent");
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert!(view.message("t3").is_none());

    push(&client, text_step("the moon"));
    let thread = nth_thread(1);
    let resumed = finished(
        call_durable(
            &host,
            &graph,
            opening(&[said("i1", "where")]),
            thread.as_str(),
            "r1",
        )
        .await,
    );

    assert_eq!(resumed.str(&key("answer")).unwrap(), "the moon");
    assert_eq!(last_turn(&resumed).id(), &named_turn("t3"));
    let view = store.view(&thread).unwrap();
    assert_eq!(view.message("t3").unwrap().status, MessageStatus::Completed);
    assert_eq!(
        store_lines(&store, "t2")
            .iter()
            .filter(|line| line.starts_with("end"))
            .count(),
        1,
        "the failed turn is not ended again"
    );
}
