//! A durable generator-critic whose generator turn was failed, under a reply
//! the client refused or by a crash: the failed turn is no generation, in
//! the run that failed it and on entry alike. It stays in the conversation,
//! is neither judged nor counted, and the generator answers again in the
//! turn of the next rank.

use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::{TurnId, TurnItem};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Count, Graph};
use crate::react::MessageStatus;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::{call_on, durable_duo};
use crate::react::generator_critic_support::*;
use crate::react::react_durable_support::{Host, nth_thread, opening, said};
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;
use crate::testkit::{Op, TestStore};

/// A generator reply cut after two events, refused as malformed.
fn cut(client: &MockClient, partial: &str) {
    client.push_reply(
        Reply::cut(
            text_step(partial),
            2,
            ClientError::MalformedResponse {
                detail: "a broken stream".to_owned(),
            },
        )
        .build()
        .unwrap(),
    );
}

/// The durable generator-critic on the two clients in `store`, its
/// generator with `echo` and one reply retry.
fn retrying(
    generator_client: &MockClient,
    critic_client: &MockClient,
    store: &Arc<TestStore>,
) -> Arc<Graph> {
    GeneratorCritic {
        generator: GeneratorSeat {
            tools: vec![Arc::new(EchoTool)],
            reply_retries: Some(Count::Fixed(1)),
            ..generator(generator_client)
        },
        ..durable_duo(generator_client, critic_client, store)
    }
    .graph()
    .unwrap()
}

fn turn(id: &str) -> TurnId {
    TurnId::new(id).unwrap()
}

#[tokio::test]
async fn given_a_generator_reply_refused_on_a_later_round_when_a_retry_is_left_then_the_failed_turn_is_no_generation_and_the_retry_opens_the_next_rank()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    cut(&generator_client, "echo sa");
    push(&generator_client, text_step("echo said echoed"));
    let graph = retrying(
        &generator_client,
        &mock(vec![verdict(true, "Good.")]),
        &store,
    );

    let done = ended(
        call_on(
            &Host::new(),
            &graph,
            opening(&[said("i1", "echo")]),
            "",
            "r1",
        )
        .await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "echo said echoed");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    let view = store.view(&nth_thread(1)).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(view.message("t3").unwrap().status, MessageStatus::Completed);
    let failed = view.conversation.turn(&turn("t2")).unwrap();
    assert!(
        failed
            .items()
            .iter()
            .any(|item| matches!(item, TurnItem::ToolResults(_))),
        "the round it answered stays in the failed turn"
    );
}

#[tokio::test]
async fn given_a_crash_after_a_generator_turn_is_failed_and_before_the_next_is_opened_when_the_call_runs_again_then_the_generator_answers_again_in_the_next_rank()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(Vec::new());
    cut(&generator_client, "a dra");
    let critic_client = mock(vec![verdict(true, "Good.")]);
    let graph = retrying(&generator_client, &critic_client, &store);
    let host = Host::new();
    store.fail(Op::OpenTurn, 2);

    let (_, error) = failed_in(
        call_on(&host, &graph, opening(&[said("i1", "explain")]), "", "r1").await,
        "generate",
    );
    assert!(matches!(error, GraphError::Thread { .. }));
    assert_eq!(generator_client.calls(), 1, "the retry was never sent");

    push(&generator_client, text_step("a draft"));
    let thread = nth_thread(1);
    let done = ended(
        call_on(
            &host,
            &graph,
            opening(&[said("i1", "explain")]),
            thread.as_str(),
            "r1",
        )
        .await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "a draft");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(generator_client.calls(), 2);
    let judged = requests(&critic_client);
    assert!(
        format!("{:?}", judged[0].messages()).contains("a draft"),
        "the critic judges the generation of the next rank"
    );
    let view = store.view(&thread).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert!(
        view.conversation.turn(&turn("t2")).is_some(),
        "it stays in the thread"
    );
    assert_eq!(view.message("t3").unwrap().status, MessageStatus::Completed);
}

#[tokio::test]
async fn given_a_generation_a_crash_cut_while_it_streamed_when_the_call_resumes_then_it_is_no_generation_and_the_generator_answers_again()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(Vec::new());
    cut(&generator_client, "a dra");
    let critic_client = mock(vec![verdict(true, "Good.")]);
    let graph = GeneratorCritic {
        generator: generator(&generator_client),
        ..durable_duo(&generator_client, &critic_client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    failed_in(
        call_on(&host, &graph, opening(&[said("i1", "explain")]), "", "r1").await,
        "generate",
    );
    let thread = nth_thread(1);
    let left = store.view(&thread).unwrap();
    assert_eq!(left.message("t2").unwrap().status, MessageStatus::Streaming);

    push(&generator_client, text_step("a draft"));
    let done = ended(
        call_on(
            &host,
            &graph,
            opening(&[said("i1", "explain")]),
            thread.as_str(),
            "r1",
        )
        .await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "a draft");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(
        shape(done.transcript()),
        ["user", "writer:a dra", "writer:a draft", "reviewer:Good."],
        "the cut generation stays in the conversation, unjudged"
    );
    let view = store.view(&thread).unwrap();
    assert_eq!(view.message("t2").unwrap().status, MessageStatus::Failed);
    assert_eq!(critic_client.calls(), 1);
}

#[tokio::test]
async fn given_a_call_whose_generation_was_retried_in_the_next_rank_when_it_runs_again_then_its_outputs_come_from_the_thread_and_the_failed_turn_is_no_generation()
 {
    let store = Arc::new(TestStore::new());
    let generator_client = mock(Vec::new());
    cut(&generator_client, "a dra");
    push(&generator_client, text_step("a draft"));
    let graph = retrying(
        &generator_client,
        &mock(vec![verdict(true, "Good.")]),
        &store,
    );
    let host = Host::new();
    let first = ended(
        call_on(&host, &graph, opening(&[said("i1", "explain")]), "", "r1").await,
        "validated",
    );
    assert_eq!(first.int(&key("nb_generations")).unwrap(), 1);

    let thread = nth_thread(1);
    let again = ended(
        call_on(
            &host,
            &graph,
            opening(&[said("i1", "explain")]),
            thread.as_str(),
            "r1",
        )
        .await,
        "validated",
    );

    assert_eq!(again.str(&key("answer")).unwrap(), "a draft");
    assert_eq!(
        again.int(&key("nb_generations")).unwrap(),
        1,
        "as the run that retried it counted"
    );
    assert_eq!(generator_client.calls(), 2, "no model is called again");
}
