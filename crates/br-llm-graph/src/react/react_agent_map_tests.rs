//! Durable agents as the body of a map with a concurrency limit: each item
//! is a call of its own, with its own client, its own thread, and at most
//! the limit of calls run at once.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::Client;
use br_llm_clients_mock::{Gate, MockClient, Reply};
use br_llm_messages::Entry;

use crate::graph::{
    Always, GraphBuilder, Input, ItemFailure, Limit, Map, Output, SubGraph, Target,
};
use crate::react::given::{Model, Reads};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::{Host, opening, said};
use crate::react::test_support::*;
use crate::run::{Outcome, control, run};
use crate::state::{Config, Kind, Schema, Value};
use crate::testkit::TestStore;
use crate::value::EndLabel;

const ITEMS: usize = 4;
const LIMIT: usize = 2;

/// The client of item `index`, answering `ans{index}` once the gate opens.
fn held(index: usize, gate: &Gate) -> MockClient {
    let client = mock(Vec::new());
    client.push_reply(
        Reply::success(text_step(&format!("ans{index}")))
            .gate_at(0, gate)
            .build()
            .unwrap(),
    );
    client
}

/// A model giving each call the client of its item, told by the id of the
/// first user input of its conversation (`q{index}`).
fn per_item(clients: &[MockClient]) -> Model<ReactAgent> {
    let clients: Vec<Arc<dyn Client>> = clients
        .iter()
        .map(|client| Arc::new(client.clone()) as Arc<dyn Client>)
        .collect();
    Model::from_fn(Reads::new(), move |ctx| {
        let index = ctx
            .conversation()
            .entries()
            .iter()
            .find_map(|entry| match entry {
                Entry::UserInput(input) => input.id().as_str().strip_prefix('q')?.parse().ok(),
                Entry::Turn(_) => None,
            })
            .unwrap_or(0);
        Ok(clients[index].clone())
    })
}

#[tokio::test]
async fn given_durable_agents_as_a_map_body_with_a_concurrency_limit_when_run_then_one_client_and_one_thread_per_item_at_most_the_limit_at_once()
 {
    let store = Arc::new(TestStore::new());
    let gate = Gate::new();
    let clients: Vec<MockClient> = (0..ITEMS).map(|index| held(index, &gate)).collect();
    let agent_graph = ReactAgent {
        model: per_item(&clients),
        thread_store: Some(store.clone()),
        ..agent(&clients[0])
    }
    .graph()
    .unwrap();
    let schema = Schema::builder()
        .state_by_name(key("questions"), Kind::list(Kind::Conversation))
        .state_by_name(key("question"), Kind::Conversation)
        .state_by_name(key("answers"), Kind::list(Kind::Str))
        .state_by_name(key("threads"), Kind::list(Kind::Str))
        .build();
    let body = SubGraph::call(agent_graph)
        .input(key("opening"), Input::From(key("question")))
        .input(key("thread"), Input::Const(Value::str("")))
        .config(key("base"), Input::Const(Value::str("Be brief.")))
        .output(key("answer"), Output::Append(key("answers")))
        .output(key("thread"), Output::Append(key("threads")));
    let graph = GraphBuilder::new(schema.clone())
        .entry(nid("each"))
        .map(
            nid("each"),
            Map {
                list: key("questions"),
                item: key("question"),
                body: Box::new(body),
                max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(LIMIT).unwrap())),
                on_item_failure: ItemFailure::Finish,
            },
        )
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .input(key("questions"))
        .build()
        .unwrap();
    let questions = (0..ITEMS)
        .map(|index| Value::conversation(opening(&[said(&format!("q{index}"), "go")])))
        .collect();
    let state = graph
        .start_state([(key("questions"), Value::list(questions))])
        .unwrap();
    let config = Config::new(&schema, BTreeMap::new()).unwrap();
    let host = Host::new();
    let ctx = host.ctx("r1");
    let (_sender, mut channel) = control();
    let calls = || clients.iter().map(MockClient::calls).sum::<usize>();
    let check = async {
        gate.wait_for_waiting(LIMIT).await;
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        let started = calls();
        gate.open();
        started
    };

    let (outcome, started) =
        futures_util::join!(run(&graph, &config, state, None, &ctx, &mut channel), check);

    assert_eq!(
        started, LIMIT,
        "no call begins past the limit while the others are held"
    );
    let Outcome::Finished { state, .. } = outcome.map_err(|failure| failure.error).unwrap() else {
        panic!("expected finished");
    };
    let answers: Vec<Value> = (0..ITEMS)
        .map(|index| Value::str(format!("ans{index}")))
        .collect();
    assert_eq!(state.list(&key("answers")).unwrap(), answers.as_slice());
    assert!(
        clients.iter().all(|client| client.calls() == 1),
        "one client per item"
    );
    let threads: BTreeSet<String> = state
        .list(&key("threads"))
        .unwrap()
        .iter()
        .map(|thread| match thread {
            Value::Str(id) => id.clone(),
            other => panic!("expected a thread id, got {other:?}"),
        })
        .collect();
    assert_eq!(threads.len(), ITEMS, "a thread per item: {threads:?}");
    for thread in &threads {
        assert!(
            store
                .view(&crate::value::ThreadId::new(thread).unwrap())
                .is_some()
        );
    }
}
