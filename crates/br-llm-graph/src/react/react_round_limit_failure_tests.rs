//! The round limit across a failed turn of a ReAct agent: a turn that
//! follows one the thread holds failed or cancelled continues that turn's
//! count, so a failure never resets the limit. A reply refused after it
//! streamed (with a retry left) and a crash both fail the turn: the agent
//! then counts as it does in memory, where the retry stays in the turn.

use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::StopReason;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Count, Limit};
use crate::react::MessageStatus;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::{Called, agent, call, finished, last_turn, nid, question};
use crate::react::react_durable_support::*;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;
use crate::testkit::TestStore;

/// Adds to `client` a reply cut after two events and refused as unusable.
fn refused_after_streaming(client: &MockClient) {
    let refused = ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    };
    client.push_reply(
        Reply::cut(text_step("echo sa"), 2, refused)
            .build()
            .unwrap(),
    );
}

/// Adds to `client` a tool call of `echo`.
fn echo_call(client: &MockClient) {
    push(client, tool_call_step("c1", "echo", json!({})));
}

/// The script of the repro: a tool call, a reply refused after it streamed,
/// a tool call, a text.
fn repro(client: &MockClient) {
    echo_call(client);
    refused_after_streaming(client);
    echo_call(client);
    push(client, text_step("echo said echoed"));
}

/// A round limit of one round, with `on_limit`.
fn one_round(on_limit: OnLimit) -> Option<RoundLimit> {
    Some(RoundLimit {
        max_rounds: Limit::Fixed(NonZeroUsize::new(1).unwrap()),
        on_limit,
    })
}

/// The agent on `client` with one round and `on_limit`, one reply retry, in
/// memory.
fn in_memory(client: &MockClient, on_limit: OnLimit) -> ReactAgent {
    ReactAgent {
        round_limit: one_round(on_limit),
        reply_retries: Some(Count::Fixed(1)),
        ..agent(client)
    }
}

/// The same agent, durable in `store`.
fn in_thread(client: &MockClient, on_limit: OnLimit, store: &Arc<TestStore>) -> ReactAgent {
    ReactAgent {
        round_limit: one_round(on_limit),
        reply_retries: Some(Count::Fixed(1)),
        ..durable(client, store)
    }
}

fn continue_limit() -> OnLimit {
    OnLimit::Continue {
        node: nid("limit"),
        flag: key("limited"),
    }
}

fn end_limit() -> OnLimit {
    OnLimit::End {
        node: nid("limit"),
        flag: key("limited"),
    }
}

/// The error a run gave.
fn error_of(called: Called) -> GraphError {
    called.err().expect("the run fails").error
}

fn limit_reached(error: &GraphError) -> bool {
    matches!(
        error,
        GraphError::ToolLimitReached { node, max_rounds: 1 } if *node == nid("llm")
    )
}

/// Runs `graph` durable on a first question in a new thread.
async fn first_call(host: &Host, graph: &crate::graph::Graph) -> Called {
    call_durable(host, graph, opening(&[said("i1", "echo")]), "", "r1").await
}

#[tokio::test]
async fn given_the_repro_script_with_the_limit_error_when_run_in_memory_or_durable_then_both_reach_the_limit()
 {
    let memory = mock(Vec::new());
    repro(&memory);
    let graph = in_memory(&memory, OnLimit::Error).graph().unwrap();
    let error = error_of(call(&graph, question("echo")).await);
    assert!(limit_reached(&error), "{error}");
    assert_eq!(memory.calls(), 3);

    let store = Arc::new(TestStore::new());
    let thread = mock(Vec::new());
    repro(&thread);
    let graph = in_thread(&thread, OnLimit::Error, &store).graph().unwrap();
    let error = error_of(first_call(&Host::new(), &graph).await);
    assert!(limit_reached(&error), "{error}");
    assert_eq!(thread.calls(), 3, "the fourth reply is never asked");
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
}

#[tokio::test]
async fn given_the_repro_script_with_the_limit_continue_when_run_in_memory_or_durable_then_both_refuse_the_second_round_and_set_the_flag()
 {
    for store in [None, Some(Arc::new(TestStore::new()))] {
        let client = mock(Vec::new());
        repro(&client);
        let (done, label) = match &store {
            None => {
                let graph = in_memory(&client, continue_limit()).graph().unwrap();
                (finished(call(&graph, question("echo")).await), "memory")
            }
            Some(store) => {
                let graph = in_thread(&client, continue_limit(), store).graph().unwrap();
                (finished(first_call(&Host::new(), &graph).await), "thread")
            }
        };
        assert_eq!(done.str(&key("answer")).unwrap(), "echo said echoed");
        assert!(done.bool(&key("limited")).unwrap(), "{label}");
        let results = results_of(&last_turn(&done));
        let refused = results.last().unwrap();
        assert!(
            refused.1 && refused.2.starts_with("Tool round limit reached (1)"),
            "{label}: the second tool call is refused, not run: {results:?}"
        );
    }
}

#[tokio::test]
async fn given_the_repro_script_with_the_limit_end_when_run_in_memory_or_durable_then_both_close_the_turn()
 {
    for store in [None, Some(Arc::new(TestStore::new()))] {
        let client = mock(Vec::new());
        repro(&client);
        let done = match &store {
            None => {
                let graph = in_memory(&client, end_limit()).graph().unwrap();
                finished(call(&graph, question("echo")).await)
            }
            Some(store) => {
                let graph = in_thread(&client, end_limit(), store).graph().unwrap();
                finished(first_call(&Host::new(), &graph).await)
            }
        };
        assert_eq!(
            done.str(&key("answer")).unwrap(),
            "Tool round limit reached (1): no final reply."
        );
        assert!(done.bool(&key("limited")).unwrap());
        assert_eq!(
            client.calls(),
            3,
            "the turn is closed without the fourth reply"
        );
    }
}

#[tokio::test]
async fn given_a_crash_after_a_round_when_the_call_resumes_then_the_turn_of_the_next_rank_continues_the_count()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    echo_call(&client);
    client.push_reply(
        Reply::failure(ClientError::Unavailable { retry_after: None })
            .build()
            .unwrap(),
    );
    let graph = ReactAgent {
        round_limit: one_round(OnLimit::Error),
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let crashed = error_of(first_call(&host, &graph).await);
    assert!(matches!(
        failed_node(&crashed).1,
        GraphError::Model(ClientError::Unavailable { .. })
    ));

    echo_call(&client);
    push(&client, text_step("echo said echoed"));
    let error = error_of(
        call_durable(
            &host,
            &graph,
            opening(&[said("i1", "echo")]),
            nth_thread(1).as_str(),
            "r1",
        )
        .await,
    );

    assert!(limit_reached(&error), "{error}");
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(client.calls(), 3);
}

#[tokio::test]
async fn given_a_call_whose_limit_was_reached_in_a_failed_turn_when_it_is_answered_on_entry_then_the_flag_is_set()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    echo_call(&client);
    echo_call(&client);
    refused_after_streaming(&client);
    push(&client, text_step("done without tools"));
    let graph = in_thread(&client, continue_limit(), &store)
        .graph()
        .unwrap();
    let host = Host::new();
    let first = finished(first_call(&host, &graph).await);
    assert!(first.bool(&key("limited")).unwrap());
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);

    let again = finished(
        call_durable(
            &host,
            &graph,
            opening(&[said("i1", "echo")]),
            nth_thread(1).as_str(),
            "r1",
        )
        .await,
    );

    assert_eq!(again.str(&key("answer")).unwrap(), "done without tools");
    assert!(
        again.bool(&key("limited")).unwrap(),
        "the limit was reached in the failed turn the answered one continues"
    );
    assert_eq!(client.calls(), 4, "no model is called again");
}
