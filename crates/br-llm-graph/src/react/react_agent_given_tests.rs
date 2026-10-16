//! A ReAct agent built from functions of its context: the round and the
//! conversation its functions read, a history that shapes the request only,
//! and the keys its functions declare (the client of each turn:
//! `react_turn_client_tests.rs`).

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::Client;
use br_llm_messages::{Text, WireMessage};
use serde_json::json;

use crate::error::GraphError;
use crate::react::given::{History, Model, Prompt, Reads};
use crate::react::helpers::wire;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::test_support::*;
use crate::run::{control, run};
use crate::state::{Config, ConfigKey, Kind, StateKey, Value};

#[tokio::test]
async fn given_functions_reading_the_round_and_the_conversation_when_the_agent_runs_then_each_call_sees_its_own()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("done"),
    ]);
    let mut reads = Reads::new();
    let base: ConfigKey<String> = reads.config("base").unwrap();
    let graph = ReactAgent {
        system: Prompt::from_fn(reads, move |ctx| {
            Ok(format!(
                "{} round {} of {} entries",
                ctx.config(&base)?,
                ctx.round(),
                ctx.conversation().entries().len()
            ))
        }),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    finished(call(&graph, question("hi")).await);

    let systems: Vec<Option<String>> = requests(&client).iter().map(system_of).collect();
    assert_eq!(
        systems,
        [
            Some("You are a helpful agent. round 0 of 1 entries".to_owned()),
            Some("You are a helpful agent. round 1 of 2 entries".to_owned())
        ]
    );
}

#[tokio::test]
async fn given_a_history_function_when_the_agent_runs_then_it_shapes_the_request_and_the_reply_is_recorded()
 {
    let client = mock(vec![text_step("brief")]);
    let graph = ReactAgent {
        history: Some(History::from_fn(Reads::new(), |ctx| {
            let mut messages = wire(ctx.conversation(), &author())?;
            messages.push(WireMessage::Relay {
                content: vec![Text::new("Answer in one word.")?],
            });
            Ok(messages)
        })),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    let state = finished(call(&graph, question("hi")).await);

    let sent = requests(&client)[0].messages().to_vec();
    assert_eq!(sent.len(), 2, "the conversation rendered, then the relay");
    assert!(matches!(sent.last(), Some(WireMessage::Relay { .. })));
    assert_eq!(last_turn(&state).author(), Some(&author()));
    assert_eq!(
        state.transcript().entries().len(),
        2,
        "the question and the agent's turn: nothing of the relay is recorded"
    );
    assert_eq!(state.str(&key("answer")).unwrap(), "brief");
}

#[tokio::test]
async fn given_keys_its_functions_declare_when_the_agent_is_built_then_inputs_and_configuration_keys_of_their_kinds()
 {
    let small = mock(vec![text_step("from the small model")]);
    let large = mock(vec![text_step("from the large model")]);
    let mut reads = Reads::new();
    let tiers: StateKey<Vec<String>> = reads.state("tiers").unwrap();
    let budget: ConfigKey<i64> = reads.config("budget").unwrap();
    let clients: [Arc<dyn Client>; 2] = [Arc::new(small.clone()), Arc::new(large.clone())];
    let read = tiers.clone();
    let graph = ReactAgent {
        model: Model::from_fn(reads, move |ctx| {
            let large = ctx.get(&read)?.contains(&"large") && ctx.config(&budget)? > 10;
            Ok(clients[usize::from(large)].clone())
        }),
        ..agent(&small)
    }
    .graph()
    .unwrap();

    assert_eq!(
        graph.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("tiers"), Kind::list(Kind::Str)),
        ])
    );
    assert_eq!(
        graph.schema().config,
        BTreeMap::from([(key("base"), Kind::Str), (key("budget"), Kind::Int)])
    );

    let state = graph
        .start_state([
            (key("opening"), Value::conversation(question("hi"))),
            tiers
                .given(vec!["small".to_owned(), "large".to_owned()])
                .unwrap(),
        ])
        .unwrap();
    let config = Config::new(
        graph.schema(),
        BTreeMap::from([
            (key("base"), Value::str("You are a helpful agent.")),
            (key("budget"), Value::int(50)),
        ]),
    )
    .unwrap();
    let (_sender, mut channel) = control();
    let state = finished(run(&graph, &config, state, None, &ctx(), &mut channel).await);

    assert_eq!(state.str(&key("answer")).unwrap(), "from the large model");
    assert_eq!((small.calls(), large.calls()), (0, 1));
}

#[test]
fn given_a_function_that_reads_the_enabled_key_when_the_agent_is_built_then_one_input_of_its_kind_or_key_collision()
 {
    let graph = ReactAgent {
        system: Prompt::from_state_list(&skey("offered")),
        enabled: Some(key("offered")),
        ..agent(&mock(Vec::new()))
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs.get(&key("offered")),
        Some(&Kind::list(Kind::Str))
    );
    assert_eq!(graph.signature().inputs.len(), 2);

    let built = ReactAgent {
        system: Prompt::from_state(&skey("offered")),
        enabled: Some(key("offered")),
        ..agent(&mock(Vec::new()))
    }
    .graph();
    assert!(matches!(
        built,
        Err(GraphError::KeyCollision { key: ref found }) if *found == key("offered")
    ));
}

#[test]
fn given_two_functions_declaring_one_key_with_two_kinds_when_the_agent_is_built_then_key_collision()
{
    let client = mock(Vec::new());
    let built = ReactAgent {
        system: Prompt::from_state(&skey("persona")),
        history: Some(History::from_fn(
            {
                let mut reads = Reads::new();
                reads.read_state(&skey::<Vec<String>>("persona"));
                reads
            },
            |ctx| Ok(wire(ctx.conversation(), &author())?),
        )),
        ..agent(&client)
    }
    .graph();
    assert!(matches!(
        built,
        Err(GraphError::KeyCollision { key: ref found }) if *found == key("persona")
    ));
}

#[tokio::test]
async fn given_a_history_function_that_fails_when_the_agent_runs_then_the_node_fails_unsent() {
    let client = mock(vec![text_step("unused")]);
    let graph = ReactAgent {
        history: Some(History::from_fn(Reads::new(), |_| {
            Err("the archive is sealed".into())
        })),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    let failure = call(&graph, question("hi")).await.err().unwrap();

    assert!(
        failure.error.to_string().contains("the archive is sealed"),
        "{}",
        failure.error
    );
    assert_eq!(client.calls(), 0);
}
