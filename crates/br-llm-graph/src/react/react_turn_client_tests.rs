//! The client of an agent's turn: its model is asked once, at the turn's
//! first round, and the client kept for the turn's other rounds, so a turn is
//! run by one client; a schedule advances per turn.

use std::sync::{Arc, Mutex};

use br_llm_clients_core::{Client, ClientError};
use br_llm_clients_mock::Reply;
use br_llm_messages::{Text, ToolCallId, ToolName, ToolResult, ToolResultBlock, Turn, TurnId};
use serde_json::json;

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support as duo_support;
use crate::react::given::{Model, Reads};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::*;
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;
use crate::testkit::TestStore;

/// A model on `client` that records the round its context holds each time
/// it is asked.
fn asked_at(client: &Arc<dyn Client>, rounds: &Arc<Mutex<Vec<usize>>>) -> Model<ReactAgent> {
    let (client, rounds) = (client.clone(), rounds.clone());
    Model::from_fn(Reads::new(), move |ctx| {
        rounds.lock().unwrap().push(ctx.round());
        Ok(client.clone())
    })
}

#[tokio::test]
async fn given_a_turn_of_several_rounds_when_the_agent_runs_then_its_model_is_asked_once_at_the_first_round()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        tool_call_step("c2", "echo", json!({})),
        text_step("done"),
    ]);
    let shared: Arc<dyn Client> = Arc::new(client.clone());
    let rounds = Arc::new(Mutex::new(Vec::new()));
    let graph = ReactAgent {
        model: asked_at(&shared, &rounds),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    let state = finished(call(&graph, question("hi")).await);

    assert_eq!(state.str(&key("answer")).unwrap(), "done");
    assert_eq!(client.calls(), 3);
    assert_eq!(*rounds.lock().unwrap(), [0], "asked once, at round 0");
}

#[tokio::test]
async fn given_a_schedule_when_a_turn_runs_several_rounds_then_they_all_call_its_first_client() {
    let first = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done"),
    ]);
    let then = mock(Vec::new());
    let graph = ReactAgent {
        model: Model::schedule(Arc::new(first.clone()), vec![Arc::new(then.clone())]),
        ..agent(&first)
    }
    .graph()
    .unwrap();

    let state = finished(call(&graph, question("hi")).await);

    assert_eq!(state.str(&key("answer")).unwrap(), "done");
    assert_eq!((first.calls(), then.calls()), (2, 0));

    push(&first, text_step("again"));
    let state = finished(call(&graph, question("again")).await);
    assert_eq!(state.str(&key("answer")).unwrap(), "again");
    assert_eq!(
        (first.calls(), then.calls()),
        (3, 0),
        "a new call starts again from its first turn"
    );
}

#[tokio::test]
async fn given_a_durable_turn_repaired_on_entry_when_the_agent_goes_on_then_its_new_turn_calls_the_next_client_of_the_schedule()
 {
    let store = Arc::new(TestStore::new());
    let first = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    first.push_reply(
        Reply::failure(ClientError::Unavailable { retry_after: None })
            .build()
            .unwrap(),
    );
    let then = mock(vec![text_step("done")]);
    let graph = ReactAgent {
        model: Model::schedule(Arc::new(first.clone()), vec![Arc::new(then.clone())]),
        ..durable(&first, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "hi")]);

    failure(call_durable(&host, &graph, question.clone(), "", "r1").await);
    assert_eq!(
        (first.calls(), then.calls()),
        (2, 0),
        "the round after the tool round keeps the turn's client"
    );

    let done = finished(call_durable(&host, &graph, question, "", "r1").await);

    assert_eq!(done.str(&key("answer")).unwrap(), "done");
    assert_eq!((first.calls(), then.calls()), (2, 1));
}

#[tokio::test]
async fn given_an_own_turn_continued_in_memory_when_the_agent_answers_then_its_model_is_asked_at_its_first_round_of_this_call()
 {
    let mut chat = question("hi");
    let mut turn = Turn::new(
        TurnId::new("t0").unwrap(),
        Some(author()),
        tool_call_step("c0", "echo", json!({})),
    );
    turn.push_result(ToolResult::new(
        ToolCallId::new("c0").unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    ))
    .unwrap();
    chat.push_turn(turn).unwrap();
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done"),
    ]);
    let shared: Arc<dyn Client> = Arc::new(client.clone());
    let rounds = Arc::new(Mutex::new(Vec::new()));
    let graph = ReactAgent {
        model: asked_at(&shared, &rounds),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    let state = finished(call(&graph, chat).await);

    assert_eq!(state.str(&key("answer")).unwrap(), "done");
    assert_eq!(client.calls(), 2);
    assert_eq!(
        *rounds.lock().unwrap(),
        [1],
        "asked once, at the round the continued turn stands at: one round already answered"
    );
}

#[tokio::test]
async fn given_a_generator_whose_generations_run_several_rounds_when_run_then_its_model_is_asked_once_per_generation()
 {
    let generator_client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("a"),
        tool_call_step("c2", "echo", json!({})),
        text_step("b"),
    ]);
    let critic_client = mock(vec![
        duo_support::verdict(false, "fix it"),
        duo_support::verdict(true, ""),
    ]);
    let shared: Arc<dyn Client> = Arc::new(generator_client.clone());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = asked.clone();
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model: Model::from_fn(Reads::new(), move |ctx| {
                seen.lock().unwrap().push((ctx.generation(), ctx.round()));
                Ok(shared.clone())
            }),
            tools: vec![Arc::new(EchoTool)],
            ..duo_support::generator(&generator_client)
        },
        duo_support::critic(&critic_client),
        Count::Fixed(2),
    )
    .graph()
    .unwrap();

    let state = duo_support::ended(
        duo_support::call(&graph, duo_support::question("explain")).await,
        "validated",
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "b");
    assert_eq!(generator_client.calls(), 4);
    assert_eq!(*asked.lock().unwrap(), [(1, 0), (2, 0)]);
}
