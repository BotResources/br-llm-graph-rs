//! What the ReAct agent tests share: the agent on a mock client, its
//! configuration, and the calls that run it.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_clients_mock::MockClient;
use br_llm_messages::{Conversation, Entry, InputId, Text, Turn, UserBlock, UserInput, UserSource};

use crate::graph::{Context, Graph, Limit};
use crate::observe::NoopObserver;
use crate::react::react_agent::ReactAgent;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, State, Value};
use crate::testkit::SeqIds;
use crate::value::NodeId;

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn question(body: &str) -> Conversation {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            InputId::new(format!("i-{body}")).unwrap(),
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new(body).unwrap())],
        )
        .unwrap(),
    )
    .unwrap();
    chat
}

pub(crate) fn agent(client: &MockClient) -> ReactAgent {
    ReactAgent {
        author: author(),
        client: Arc::new(client.clone()),
        system: vec![Source::Config(key("base"))],
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(EchoTool)],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
    }
}

pub(crate) fn limited(client: &MockClient, max_rounds: usize, on_limit: OnLimit) -> ReactAgent {
    ReactAgent {
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(max_rounds).unwrap()),
            on_limit,
        }),
        ..agent(client)
    }
}

pub(crate) fn agent_config(graph: &Graph) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a helpful agent."));
    Config::new(graph.schema(), values).unwrap()
}

pub(crate) fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

pub(crate) async fn call(graph: &Graph, history: Conversation) -> Result<Outcome, RunFailure> {
    call_in(graph, history, &ctx()).await
}

pub(crate) async fn run_agent(graph: &Graph, state: State) -> Result<Outcome, RunFailure> {
    let (_sender, mut inbox) = channel();
    run(graph, &agent_config(graph), state, None, &ctx(), &mut inbox).await
}

pub(crate) async fn call_in(
    graph: &Graph,
    history: Conversation,
    ctx: &Context,
) -> Result<Outcome, RunFailure> {
    let state = graph
        .start_state([(key("history"), Value::conversation(history))])
        .unwrap();
    let (_sender, mut inbox) = channel();
    run(graph, &agent_config(graph), state, None, ctx, &mut inbox).await
}

pub(crate) fn finished(result: Result<Outcome, RunFailure>) -> State {
    match result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, end } => {
            assert_eq!(end.as_str(), "done");
            state
        }
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

pub(crate) fn last_turn(state: &State) -> Turn {
    match state
        .conversation(&key("history"))
        .unwrap()
        .entries()
        .last()
    {
        Some(Entry::Turn(turn)) => turn.clone(),
        Some(Entry::UserInput(_)) | None => panic!("expected the agent's turn last"),
    }
}
