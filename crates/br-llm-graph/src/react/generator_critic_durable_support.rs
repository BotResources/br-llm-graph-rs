//! What the durable generator-critic tests share: the generator-critic on a
//! test store, and its calls under a run id with an id source the test keeps
//! across runs.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::ClientError;
use br_llm_clients_mock::{MockClient, Reply};
use br_llm_messages::Conversation;

use crate::graph::Graph;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_support::{Ran, duo, run_with_channel};
use crate::react::react_durable_support::Host;
use crate::react::test_support::key;
use crate::run::{Control, control};
use crate::state::{Config, State, Value};
use crate::testkit::TestStore;

/// The generator-critic on the two clients, durable in `store`.
pub(crate) fn durable_duo(
    generator_client: &MockClient,
    critic_client: &MockClient,
    store: &Arc<TestStore>,
) -> GeneratorCritic {
    GeneratorCritic {
        thread_store: Some(store.clone()),
        ..duo(generator_client, critic_client)
    }
}

/// The start state of a durable generator-critic: `opening` and `thread`.
pub(crate) fn durable_start(graph: &Graph, opening: Conversation, thread: &str) -> State {
    graph
        .start_state([
            (key("opening"), Value::conversation(opening)),
            (key("thread"), Value::str(thread)),
        ])
        .unwrap()
}

/// Runs `graph` on `opening` in `thread` under the run id `run`.
pub(crate) async fn call_on(
    host: &Host,
    graph: &Graph,
    opening: Conversation,
    thread: &str,
    run: &str,
) -> Ran {
    let (_sender, mut channel) = control();
    call_listening(host, graph, opening, thread, run, &mut channel).await
}

/// `call_on`, listening to `channel`.
pub(crate) async fn call_listening(
    host: &Host,
    graph: &Graph,
    opening: Conversation,
    thread: &str,
    run: &str,
    channel: &mut Control,
) -> Ran {
    let state = durable_start(graph, opening, thread);
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    run_with_channel(graph, state, &config, &host.ctx(run), channel).await
}

/// Makes the next call of `client` fail with `Unavailable`.
pub(crate) fn unavailable(client: &MockClient) {
    client.push_reply(
        Reply::failure(ClientError::Unavailable { retry_after: None })
            .build()
            .unwrap(),
    );
}

/// The writes of the store's log, reads left out.
pub(crate) fn writes(store: &TestStore) -> Vec<String> {
    store
        .log()
        .into_iter()
        .filter(|line| !line.starts_with("read "))
        .collect()
}
