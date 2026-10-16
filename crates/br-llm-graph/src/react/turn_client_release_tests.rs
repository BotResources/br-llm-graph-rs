//! The client kept for an agent's turn is let go: at the turn's end, before
//! the run goes on, and with the run, however it ends (paused and never
//! resumed, cancelled, failed), counted by the client's strong count.

use std::sync::{Arc, Mutex};

use br_llm_clients_core::Client;
use serde_json::json;

use crate::graph::Count;
use crate::origin::RunId;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::unavailable;
use crate::react::generator_critic_support as duo;
use crate::react::given::{Model, Reads};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::*;
use crate::run::{ControlSender, Outcome, control, run};
use crate::state::Value;

#[tokio::test]
async fn given_a_generation_that_ended_when_the_critic_judges_it_then_its_client_is_no_longer_kept()
{
    let generator = mock(vec![text_step("draft")]);
    let shared: Arc<dyn Client> = Arc::new(generator.clone());
    let probe = shared.clone();
    let critic: Arc<dyn Client> = Arc::new(mock(vec![duo::verdict(true, "")]));
    let held = Arc::new(Mutex::new(Vec::new()));
    let seen = (held.clone(), probe.clone());
    let graph = GeneratorCritic::new(
        GeneratorSeat {
            model: Model::client(shared),
            ..duo::generator(&generator)
        },
        crate::react::seats::CriticSeat {
            model: Model::from_fn(Reads::new(), move |_| {
                seen.0.lock().unwrap().push(Arc::strong_count(&seen.1));
                Ok(critic.clone())
            }),
            ..duo::critic(&mock(Vec::new()))
        },
        Count::Fixed(1),
    )
    .graph()
    .unwrap();
    // The probe, its copy in the critic's function, the generator's model.
    let base = Arc::strong_count(&probe);

    duo::ended(
        duo::call(&graph, duo::question("explain")).await,
        "validated",
    );

    assert_eq!(*held.lock().unwrap(), [base], "nothing else holds it");
}

/// A model on `client` that sends `stop` through `sender` the first time it
/// is asked, so the run stops in the middle of its first turn.
fn stopping(
    client: &Arc<dyn Client>,
    sender: ControlSender,
    stop: fn(&ControlSender),
) -> Model<ReactAgent> {
    let client = client.clone();
    Model::from_fn(Reads::new(), move |_| {
        stop(&sender);
        Ok(client.clone())
    })
}

/// Runs a ReAct agent whose first turn calls `echo` and whose model sends
/// `stop` on its first ask; gives the outcome and whether the client is
/// held beyond the graph's own copy once the run is over.
async fn stopped_mid_turn(
    stop: fn(&ControlSender),
) -> (Result<Outcome, crate::run::RunFailure>, usize, usize) {
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    let shared: Arc<dyn Client> = Arc::new(client.clone());
    let (sender, mut channel) = control();
    let graph = ReactAgent {
        model: stopping(&shared, sender, stop),
        ..agent(&client)
    }
    .graph()
    .unwrap();
    let base = Arc::strong_count(&shared);
    let state = graph
        .start_state([(key("opening"), Value::conversation(question("hi")))])
        .unwrap();
    let outcome = run(
        &graph,
        &agent_config(&graph),
        state,
        None,
        &ctx(),
        &mut channel,
    )
    .await;
    let after = Arc::strong_count(&shared);
    (outcome, base, after)
}

#[tokio::test]
async fn given_a_run_paused_mid_turn_when_it_is_never_resumed_then_it_keeps_no_client() {
    let (outcome, base, after) = stopped_mid_turn(ControlSender::pause).await;

    assert!(matches!(outcome, Ok(Outcome::Paused { .. })));
    assert_eq!(after, base);
}

#[tokio::test]
async fn given_a_run_cancelled_mid_turn_when_it_returns_then_it_keeps_no_client() {
    let (outcome, base, after) = stopped_mid_turn(ControlSender::cancel).await;

    assert!(matches!(outcome, Ok(Outcome::Cancelled { .. })));
    assert_eq!(after, base);
}

#[tokio::test]
async fn given_runs_that_fail_mid_turn_when_they_return_then_none_keeps_its_client() {
    let client = mock(Vec::new());
    let shared: Arc<dyn Client> = Arc::new(client.clone());
    let graph = ReactAgent {
        model: Model::client(shared.clone()),
        ..agent(&client)
    }
    .graph()
    .unwrap();
    let base = Arc::strong_count(&shared);

    for n in 0..5 {
        push(&client, tool_call_step(&format!("c{n}"), "echo", json!({})));
        unavailable(&client);
        let ctx = ctx().with_run_id(RunId::new(format!("run-{n}")));
        assert!(call_in(&graph, question("hi"), &ctx).await.err().is_some());
    }

    assert_eq!(
        Arc::strong_count(&shared),
        base,
        "no run keeps a client once over"
    );
}
