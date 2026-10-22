//! What the round-limit tests share: a ReAct loop on `chat` with `echo`,
//! its schema and configuration, and readings of the agent's turn.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_clients_mock::MockClient;
use br_llm_messages::{
    Conversation, Entry, InputId, Text, ToolResultBlock, Turn, TurnItem, UserBlock, UserInput,
    UserSource,
};

use crate::graph::{Context, Graph, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::given::Model;
use crate::react::llm_node::LlmNode;
use crate::react::react_agent::ReactAgent;
use crate::react::react_loop::ReactLoop;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::{
    EchoTool, TURN_RANK, author, base, first_turn, in_memory_call, key, requests, tool_names,
};
use crate::run::{Outcome, RunFailure, control, run};
use crate::state::{Config, Kind, Schema, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("chat"), Kind::Conversation)
        .state_by_name_with_default(key(TURN_RANK), Kind::Int, first_turn())
        .state_by_name(key("log"), Kind::list(Kind::Str))
        .state_by_name(key("limited"), Kind::Bool)
        .config_by_name(key("base"), Kind::Str)
        .config_by_name(key("max_rounds"), Kind::Int)
        .build()
}

pub(crate) fn config(max_rounds: i64) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a helpful agent."));
    values.insert(key("max_rounds"), Value::int(max_rounds));
    Config::new(&schema(), values).unwrap()
}

pub(crate) fn human(body: &str) -> UserInput {
    UserInput::new(
        InputId::new(format!("i-{body}")).unwrap(),
        UserSource::Human,
        None,
        vec![UserBlock::text(Text::new(body).unwrap())],
    )
    .unwrap()
}

pub(crate) fn state_with(chat: Conversation) -> State {
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("log"), Value::list(Vec::new()));
    values.insert(key("limited"), Value::bool(false));
    values.insert(key(TURN_RANK), first_turn());
    State::new(schema(), values).unwrap()
}

pub(crate) fn fresh_state() -> State {
    let mut chat = Conversation::new();
    chat.push_input(human("hi")).unwrap();
    state_with(chat)
}

/// The tools each call of `client` declared, in call order.
pub(crate) fn declared(client: &MockClient) -> Vec<Vec<String>> {
    requests(client).iter().map(tool_names).collect()
}

pub(crate) fn graph(client: &MockClient, max_rounds: Limit, on_limit: OnLimit) -> Graph {
    let llm = LlmNode::<ReactAgent> {
        key: key("chat"),
        author: author(),
        model: Model::client(Arc::new(client.clone())),
        system: base(),
        history: None,
        tools: vec![Arc::new(EchoTool)],
        enabled: None,
        output_mode: OutputMode::Plain,
        tool_concurrency: None,
        round_limit: Some(RoundLimit {
            max_rounds,
            on_limit,
        }),
        call: in_memory_call(),
        seat: None,
    };
    let react = ReactLoop {
        llm: nid("llm"),
        tools_node: nid("tools"),
        split: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
        thread: None,
    };
    react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap()
}

pub(crate) async fn run_graph(
    graph: &Graph,
    config: Config,
    state: State,
) -> Result<Outcome, RunFailure> {
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut channel) = control();
    run(graph, &config, state, None, &ctx, &mut channel).await
}

pub(crate) fn finished(result: Result<Outcome, RunFailure>) -> State {
    match result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, .. } => state,
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

pub(crate) fn own_turn(state: &State) -> Turn {
    match state.conversation(&key("chat")).unwrap().entries().last() {
        Some(Entry::Turn(turn)) => turn.clone(),
        Some(Entry::UserInput(_)) | None => panic!("expected the agent's turn last"),
    }
}

pub(crate) fn result_texts(turn: &Turn) -> Vec<(String, bool)> {
    let mut texts = Vec::new();
    for item in turn.items() {
        if let TurnItem::ToolResults(results) = item {
            for result in results.results() {
                for block in &result.content {
                    let ToolResultBlock::Text { text } = block else {
                        continue;
                    };
                    texts.push((text.as_str().to_owned(), result.is_error));
                }
            }
        }
    }
    texts
}

/// The error result each call gets once a limit of 1 is reached.
pub(crate) const CONTINUE_REFUSAL: &str =
    "Tool round limit reached (1): this call was not executed. Reply without calling tools.";

pub(crate) fn continue_limit() -> OnLimit {
    OnLimit::Continue {
        node: nid("limit"),
        flag: key("limited"),
    }
}

pub(crate) fn two() -> Limit {
    Limit::Fixed(NonZeroUsize::new(2).unwrap())
}
