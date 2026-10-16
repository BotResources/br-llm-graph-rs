//! A session serving a durable agent's graph, given an input into `opening`
//! while the agent is in the middle of its turn: the input is held and
//! applied after that superstep, the turn goes on undisturbed, and the
//! thread does not hold the late input, which the agent's entry had not
//! read.

use std::sync::Arc;

use br_llm_clients_mock::{Gate, Reply, StateKind};

use crate::react::react_agent_support::agent_config;
use crate::react::react_durable_support::*;
use crate::react::test_support::*;
use crate::session::{Ended, Session, Start};
use crate::testkit::TestStore;

#[tokio::test]
async fn given_a_session_serving_a_durable_agent_when_an_input_arrives_mid_turn_then_it_is_held_and_applied_and_the_turn_goes_on()
 {
    let store = Arc::new(TestStore::new());
    let gate = Gate::new();
    let client = mock(Vec::new());
    client.push_reply(
        Reply::success(text_step("final"))
            .gate_at(1, &gate)
            .build()
            .unwrap(),
    );
    let graph = durable(&client, &store).graph().unwrap();
    let state = start(&graph, opening(&[said("i1", "hello")]), "");
    let host = Host::new();
    let session = Session::new(graph.clone(), agent_config(&graph), state, host.ctx("r1")).unwrap();
    let sender = session.sender();
    let drive = async {
        gate.wait_for_waiting(1).await;
        sender.send(key("opening"), said("i2", "late"));
        gate.open();
        client
            .wait_for_state(StateKind::Succeeded, 1)
            .unwrap()
            .await;
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        sender.cancel();
    };

    let ended = match beside(session.serve(Start::Now), drive).await {
        Ok((ended, ())) => ended,
        Err(Ended::Failed { error, .. }) => panic!("serving failed before the gate: {error}"),
        Err(_) => panic!("serving ended before the gate"),
    };

    let Ended::Cancelled { checkpoint } = ended else {
        panic!("expected serving to end on the cancel");
    };
    assert!(checkpoint.cursor.is_empty(), "the run had finished");
    assert_eq!(checkpoint.state.str(&key("answer")).unwrap(), "final");
    let opening = checkpoint.state.conversation(&key("opening")).unwrap();
    assert_eq!(
        opening.entries().len(),
        2,
        "the late input was held, then applied"
    );
    assert_eq!(
        shape_of_thread(&store),
        ["user", "agent:final"],
        "the thread holds the trigger and the turn, not the late input"
    );
    assert_eq!(client.calls(), 1);
    let json = serde_json::to_value(&checkpoint).unwrap();
    assert!(
        json["state"]["values"].get("conversation").is_none(),
        "{json}"
    );
}

/// The thread of the first call, each message as `user` or `author:text`.
fn shape_of_thread(store: &TestStore) -> Vec<String> {
    crate::react::generator_critic_support::shape(&store.view(&nth_thread(1)).unwrap().conversation)
}
