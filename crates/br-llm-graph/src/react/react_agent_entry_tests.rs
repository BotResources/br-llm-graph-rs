//! Where a ReAct agent enters: its signature in memory and durable, an
//! agent in memory started again from its opening when its graph resumes
//! from a checkpoint, and the round counted from 0 in the turn a durable
//! agent opens after a repaired one.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::Reply;
use serde_json::json;

use crate::error::GraphError;
use crate::react::given::{Prompt, Reads};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::*;
use crate::react::test_support::*;
use crate::run::control;
use crate::state::{ConfigKey, Kind};
use crate::testkit::TestStore;

#[test]
fn given_a_durable_agent_when_built_then_thread_is_an_input_and_an_output_and_a_key_it_has() {
    let store = Arc::new(TestStore::new());
    let client = mock(Vec::new());
    let graph = ReactAgent {
        inputs: Vec::new(),
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    assert_eq!(
        graph.signature().inputs,
        BTreeMap::from([
            (key("opening"), Kind::Conversation),
            (key("thread"), Kind::Str),
        ])
    );
    assert_eq!(
        graph.signature().outputs,
        BTreeMap::from([(key("answer"), Kind::Str), (key("thread"), Kind::Str)])
    );
    for private in ["conversation", "turn_rank"] {
        assert!(!graph.schema().state.contains_key(&key(private)));
    }

    let built = ReactAgent {
        inputs: vec![(key("thread"), Kind::Str)],
        ..durable(&client, &store)
    }
    .graph();
    assert!(
        matches!(built, Err(GraphError::KeyCollision { key: found }) if found == key("thread"))
    );
    let in_memory = ReactAgent {
        inputs: vec![(key("thread"), Kind::Str)],
        ..agent(&client)
    };
    assert!(
        in_memory.graph().is_ok(),
        "an agent in memory has no thread"
    );
}

#[tokio::test]
async fn given_an_agent_in_memory_resumed_from_its_failure_checkpoint_when_it_runs_then_it_starts_again_from_its_opening()
 {
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    client.push_reply(
        Reply::failure(ClientError::Unavailable { retry_after: None })
            .build()
            .unwrap(),
    );
    push(&client, text_step("done"));
    let graph = agent(&client).graph().unwrap();

    let failed = call(&graph, question("hi")).await.err().unwrap();
    assert_eq!(failed.checkpoint.cursor.active, [nid("enter")]);
    assert!(failed.checkpoint.pending.is_empty());
    assert!(
        !failed
            .checkpoint
            .state
            .schema()
            .state
            .contains_key(&key("conversation"))
    );

    let (_sender, mut channel) = control();
    let resumed = crate::run::run(
        &graph,
        &agent_config(&graph),
        failed.checkpoint.state,
        Some(failed.checkpoint.cursor),
        &ctx(),
        &mut channel,
    )
    .await;
    let done = finished(resumed);
    assert_eq!(done.str(&key("answer")).unwrap(), "done");
    let requests = requests(&client);
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[2].messages(),
        requests[0].messages(),
        "the opening alone, as on the first call"
    );
}

#[tokio::test]
async fn given_a_durable_turn_repaired_on_entry_when_the_agent_goes_on_then_the_round_of_its_new_turn_starts_from_zero()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    client.push_reply(
        Reply::failure(ClientError::Unavailable { retry_after: None })
            .build()
            .unwrap(),
    );
    push(&client, text_step("done"));
    let mut reads = Reads::new();
    let base: ConfigKey<String> = reads.config("base").unwrap();
    let graph = ReactAgent {
        system: Prompt::from_fn(reads, move |ctx| {
            Ok(format!("{} round {}", ctx.config(&base)?, ctx.round()))
        }),
        ..durable(&client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "hi")]);

    failure(call_durable(&host, &graph, question.clone(), "", "r1").await);
    finished(call_durable(&host, &graph, question, "", "r1").await);

    let systems: Vec<Option<String>> = requests(&client).iter().map(system_of).collect();
    assert_eq!(
        systems,
        [
            Some("You are a helpful agent. round 0".to_owned()),
            Some("You are a helpful agent. round 1".to_owned()),
            Some("You are a helpful agent. round 0".to_owned()),
        ]
    );
}
