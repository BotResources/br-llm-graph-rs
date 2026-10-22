//! The generator's round limit across a failed turn: a failed generator
//! turn is no generation, so the generation's next turn continues its
//! count. A reply refused after it streamed (with a retry left) and a crash
//! both fail the turn: the generator-critic then counts as it does in
//! memory, where the retry stays in the turn.

use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::StopReason;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Count, Graph, Limit};
use crate::react::MessageStatus;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::{call_on, durable_duo, unavailable};
use crate::react::generator_critic_support::*;
use crate::react::react_durable_support::{Host, nth_thread, opening, said};
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::seats::GeneratorSeat;
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

fn echo_call(client: &MockClient) {
    push(client, tool_call_step("c1", "echo", json!({})));
}

/// A tool call, a reply refused after it streamed, a tool call, a text.
fn repro(client: &MockClient) {
    echo_call(client);
    refused_after_streaming(client);
    echo_call(client);
    push(client, text_step("echo said echoed"));
}

/// The generator on `client` with `echo`, one round with `on_limit` and one
/// reply retry.
fn seat(client: &MockClient, on_limit: OnLimit) -> GeneratorSeat {
    GeneratorSeat {
        tools: vec![Arc::new(EchoTool)],
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(1).unwrap()),
            on_limit,
        }),
        reply_retries: Some(Count::Fixed(1)),
        ..generator(client)
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

/// The generator-critic of `seat` and a critic on `critic_client`, with a
/// budget of `budget`, durable in `store` when given.
fn graph(
    seat: GeneratorSeat,
    critic_client: &MockClient,
    budget: usize,
    store: Option<&Arc<TestStore>>,
) -> Arc<Graph> {
    let duo = match store {
        Some(store) => GeneratorCritic {
            generator: seat,
            max_critiques: Count::Fixed(budget),
            ..durable_duo(&mock(Vec::new()), critic_client, store)
        },
        None => GeneratorCritic::new(seat, critic(critic_client), Count::Fixed(budget)),
    };
    duo.graph().unwrap()
}

/// Runs `graph` on a first question: in memory, or in a new thread.
async fn first(graph: &Graph, durable: bool, host: &Host) -> Ran {
    if durable {
        call_on(host, graph, opening(&[said("i1", "echo")]), "", "r1").await
    } else {
        call(graph, question("echo")).await
    }
}

fn limit_reached(ran: Ran) -> bool {
    matches!(
        ran.result.err().map(|failure| failure.error),
        Some(GraphError::ToolLimitReached { node, max_rounds: 1 }) if node == nid("generate")
    )
}

#[tokio::test]
async fn given_the_repro_script_with_the_limit_error_when_run_in_memory_or_durable_then_both_reach_the_limit()
 {
    for durable in [false, true] {
        let store = Arc::new(TestStore::new());
        let client = mock(Vec::new());
        repro(&client);
        let graph = graph(
            seat(&client, OnLimit::Error),
            &mock(Vec::new()),
            1,
            durable.then_some(&store),
        );
        assert!(
            limit_reached(first(&graph, durable, &Host::new()).await),
            "durable: {durable}"
        );
        assert_eq!(client.calls(), 3, "durable: {durable}");
    }
}

#[tokio::test]
async fn given_the_repro_script_with_the_limit_continue_when_run_in_memory_or_durable_then_both_refuse_the_second_round_in_one_generation()
 {
    for durable in [false, true] {
        let store = Arc::new(TestStore::new());
        let client = mock(Vec::new());
        repro(&client);
        let graph = graph(
            seat(&client, continue_limit()),
            &mock(vec![verdict(true, "Good.")]),
            1,
            durable.then_some(&store),
        );
        let done = ended(first(&graph, durable, &Host::new()).await, "validated");
        assert_eq!(done.str(&key("answer")).unwrap(), "echo said echoed");
        assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
        assert!(done.bool(&key("limited")).unwrap(), "durable: {durable}");
    }
}

#[tokio::test]
async fn given_the_repro_script_with_the_limit_end_when_run_in_memory_or_durable_then_both_cut_the_generation()
 {
    for durable in [false, true] {
        let store = Arc::new(TestStore::new());
        let client = mock(Vec::new());
        repro(&client);
        let graph = graph(
            seat(&client, end_limit()),
            &mock(Vec::new()),
            0,
            durable.then_some(&store),
        );
        let done = ended(first(&graph, durable, &Host::new()).await, "exhausted");
        assert_eq!(done.str(&key("answer")).unwrap(), "", "a cut generation");
        assert!(done.bool(&key("limited")).unwrap(), "durable: {durable}");
        assert_eq!(client.calls(), 3, "durable: {durable}");
    }
}

#[tokio::test]
async fn given_a_crash_after_a_round_of_a_generation_when_the_call_resumes_then_the_generation_continues_the_count()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    echo_call(&client);
    unavailable(&client);
    let graph = graph(
        seat(&client, OnLimit::Error),
        &mock(Vec::new()),
        1,
        Some(&store),
    );
    let host = Host::new();
    failed_in(first(&graph, true, &host).await, "generate");

    echo_call(&client);
    push(&client, text_step("echo said echoed"));
    let resumed = call_on(
        &host,
        &graph,
        opening(&[said("i1", "echo")]),
        nth_thread(1).as_str(),
        "r1",
    )
    .await;

    assert!(limit_reached(resumed));
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(client.calls(), 3);
}

#[tokio::test]
async fn given_a_generation_whose_limit_was_reached_in_a_failed_turn_when_the_call_is_answered_on_entry_then_the_flag_is_set()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    echo_call(&client);
    echo_call(&client);
    refused_after_streaming(&client);
    push(&client, text_step("done without tools"));
    let graph = graph(
        seat(&client, continue_limit()),
        &mock(vec![verdict(true, "Good.")]),
        1,
        Some(&store),
    );
    let host = Host::new();
    let done = ended(first(&graph, true, &host).await, "validated");
    assert!(done.bool(&key("limited")).unwrap());

    let again = ended(
        call_on(
            &host,
            &graph,
            opening(&[said("i1", "echo")]),
            nth_thread(1).as_str(),
            "r1",
        )
        .await,
        "validated",
    );

    assert_eq!(again.str(&key("answer")).unwrap(), "done without tools");
    assert_eq!(again.int(&key("nb_generations")).unwrap(), 1);
    assert!(
        again.bool(&key("limited")).unwrap(),
        "the limit was reached in the failed turn the generation continues"
    );
    assert_eq!(client.calls(), 4, "no model is called again");
}
