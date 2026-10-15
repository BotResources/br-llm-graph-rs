//! What the durable ReAct agent tests share: the agent on a test store, the
//! calls that run it with a run id and an id source the test keeps across
//! runs, and readings of its thread.

use std::sync::Arc;

use br_llm_clients_mock::MockClient;
use br_llm_messages::{Conversation, InputId, Text, UserBlock, UserInput, UserSource};

use crate::error::GraphError;
use crate::graph::{Context, Graph};
use crate::observe::{NoopObserver, Observer};
use crate::origin::RunId;
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::{Called, agent, agent_config, observed};
use crate::react::test_support::key;
use crate::run::{Control, Outcome, RunFailure, control, run};
use crate::state::{Kind, State, Value};
use crate::testkit::{SeqIds, StoreError, TestStore};
use crate::value::ThreadId;

/// The agent on `client`, durable in `store`, with the extra input `log`
/// (a list of strings) its tools may append to.
pub(crate) fn durable(client: &MockClient, store: &Arc<TestStore>) -> ReactAgent {
    ReactAgent {
        thread_store: Some(store.clone()),
        inputs: vec![(key("log"), Kind::list(Kind::Str))],
        ..agent(client)
    }
}

/// A host: its id source, kept across runs, and the observer of its runs.
pub(crate) struct Host {
    pub(crate) ids: Arc<SeqIds>,
    observer: Arc<dyn Observer>,
}

impl Host {
    pub(crate) fn new() -> Self {
        Self::observed_by(Arc::new(NoopObserver))
    }

    pub(crate) fn observed_by(observer: Arc<dyn Observer>) -> Self {
        Self {
            ids: Arc::new(SeqIds::new()),
            observer,
        }
    }

    /// The context of a run under `run`.
    pub(crate) fn ctx(&self, run: &str) -> Context {
        Context::new(self.observer.clone(), self.ids.clone()).with_run_id(RunId::new(run))
    }
}

/// The input `id` of a human saying `body`.
pub(crate) fn said(id: &str, body: &str) -> UserInput {
    UserInput::new(
        InputId::new(id).unwrap(),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

/// An opening of `inputs`.
pub(crate) fn opening(inputs: &[UserInput]) -> Conversation {
    let mut opening = Conversation::new();
    for input in inputs {
        opening.push_input(input.clone()).unwrap();
    }
    opening
}

/// The start state of a durable agent's graph: `opening`, `thread` and an
/// empty `log`.
pub(crate) fn start(graph: &Graph, opening: Conversation, thread: &str) -> State {
    graph
        .start_state([
            (key("opening"), Value::conversation(opening)),
            (key("thread"), Value::str(thread)),
            (key("log"), Value::list(Vec::new())),
        ])
        .unwrap()
}

/// Runs the agent's graph on `state` under the run id `run_id`.
pub(crate) async fn run_on(host: &Host, graph: &Graph, state: State, run_id: &str) -> Called {
    let (_sender, mut channel) = control();
    run_in(host, graph, state, run_id, &mut channel).await
}

/// Runs the agent's graph on `state` under the run id `run_id`, listening to
/// `channel`.
pub(crate) async fn run_in(
    host: &Host,
    graph: &Graph,
    state: State,
    run_id: &str,
    channel: &mut Control,
) -> Called {
    let (ctx, transcript) = observed(&host.ctx(run_id));
    let result = run(graph, &agent_config(graph), state, None, &ctx, channel).await;
    Called::new(result, transcript)
}

/// Runs the agent's graph on `opening` in `thread` under the run id `run`.
pub(crate) async fn call_durable(
    host: &Host,
    graph: &Graph,
    opening: Conversation,
    thread: &str,
    run_id: &str,
) -> Called {
    run_on(host, graph, start(graph, opening, thread), run_id).await
}

/// The thread the agent's finished run gives back.
pub(crate) fn thread_of(state: &State) -> ThreadId {
    ThreadId::new(state.str(&key("thread")).unwrap()).unwrap()
}

/// The error of a failed run.
pub(crate) fn failure(called: Called) -> RunFailure {
    match called.result {
        Err(failure) => failure,
        Ok(Outcome::Finished { .. }) => panic!("expected a failure, the run finished"),
        Ok(Outcome::Paused { .. } | Outcome::Cancelled { .. }) => {
            panic!("expected a failure, the run stopped")
        }
    }
}

/// The node a run failed in and the error it returned.
pub(crate) fn failed_node(error: &GraphError) -> (&str, &GraphError) {
    match error {
        GraphError::NodeFailed { node, .. } => {
            (node.as_str(), crate::react::test_support::returned(error))
        }
        other => panic!("expected a failed node, got: {other}"),
    }
}

/// The store's own error inside a `Thread` error.
pub(crate) fn store_error(error: &GraphError) -> StoreError {
    match error {
        GraphError::Thread { source } => source.downcast_ref::<StoreError>().unwrap().clone(),
        other => panic!("expected a thread store error, got: {other}"),
    }
}

/// The lines of the store's log that name a write, reads left out.
pub(crate) fn writes(store: &TestStore) -> Vec<String> {
    store
        .log()
        .into_iter()
        .filter(|line| !line.starts_with("read "))
        .collect()
}

/// The thread id the test id source gives the n-th new request.
pub(crate) fn nth_thread(n: usize) -> ThreadId {
    ThreadId::new(format!("01900000-0000-7000-8000-{n:012x}")).unwrap()
}
