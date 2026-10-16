//! What the tests of the ReAct loop's nodes share: a tool node and a
//! model-call step on `chat`, a turn awaiting a call, and their runs.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{Client, OutputMode};
use br_llm_messages::{
    Conversation, Entry, InputId, Step, Text, Turn, TurnId, TurnItem, UserBlock, UserInput,
    UserSource,
};

use crate::graph::{Always, Context, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::given::{Model, Prompt};
use crate::react::llm_node::LlmNode;
use crate::react::react_agent::ReactAgent;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::react::tool_gate::SharedGates;
use crate::react::tool_node::ToolNode;
use crate::run::Outcome;
use crate::run::control;
use crate::run::run;
use crate::state::{State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

/// A tool node of the agent on `chat`, with its own gates.
pub(crate) fn tool_node(
    tools: Vec<Arc<dyn Tool>>,
    max_concurrency: Option<Limit>,
    enabled: Option<crate::value::Key>,
) -> ToolNode {
    ToolNode {
        key: key("chat"),
        author: author(),
        tools,
        max_concurrency,
        enabled,
        gates: Arc::new(SharedGates::default()),
        durable: None,
    }
}

/// A model-call step of the agent on `chat`, without tools.
pub(crate) fn llm_node(client: Arc<dyn Client>, system: Prompt<ReactAgent>) -> LlmNode<ReactAgent> {
    LlmNode::<ReactAgent> {
        key: key("chat"),
        author: author(),
        model: Model::client(client),
        system,
        history: None,
        tools: Vec::new(),
        enabled: None,
        output_mode: OutputMode::Plain,
        call: None,
        seat: None,
        clients: Default::default(),
    }
}

pub(crate) fn state_with_call(step: Step) -> State {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            InputId::new("i-hi").unwrap(),
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("hi").unwrap())],
        )
        .unwrap(),
    )
    .unwrap();
    chat.push_turn(Turn::new(TurnId::new("t1").unwrap(), Some(author()), step))
        .unwrap();
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("log"), Value::list(Vec::new()));
    values.insert(key("enabled"), Value::list(Vec::new()));
    State::new(schema(), values).unwrap()
}

pub(crate) async fn run_tool_node(state: State, tools: Vec<Arc<dyn Tool>>) -> State {
    let graph = GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(nid("tools"), tool_node(tools, None, None))
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let outcome = run(&graph, &config(), state, None, &ctx(), &mut channel)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    state
}

pub(crate) fn first_result_is_error(state: &State) -> Option<bool> {
    let convo = state.conversation(&key("chat")).ok()?;
    for entry in convo.entries() {
        if let Entry::Turn(turn) = entry {
            for item in turn.items() {
                if let TurnItem::ToolResults(results) = item {
                    return results.results().first().map(|r| r.is_error);
                }
            }
        }
    }
    None
}

/// The build of a graph whose single node is `node`.
pub(crate) fn built_alone(node: impl crate::graph::Node + 'static) -> crate::error::GraphError {
    crate::testkit::refusal(
        GraphBuilder::new(schema())
            .entry(nid("only"))
            .node(nid("only"), node)
            .edge(
                nid("only"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build(),
    )
}
