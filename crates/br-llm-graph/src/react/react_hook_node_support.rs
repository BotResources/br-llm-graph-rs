//! What the hook tests share around the hooks: a tool node on a pending turn
//! of the agent, the run of it, and the results it answered.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_messages::{
    AssistantBlock, Conversation, Entry, InputId, Step, StopReason, Text, ToolCall, ToolCallId,
    ToolName, Turn, TurnId, UserBlock, UserInput, UserSource,
};
use serde_json::Value as Json;

use crate::graph::{Always, Context, Graph, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::react::tool_gate::SharedGates;
use crate::react::tool_node::ToolNode;
use crate::run::{Outcome, RunFailure, control, run};
use crate::state::{State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn call(id: &str, name: &str, arguments: Json) -> AssistantBlock {
    AssistantBlock::ToolCall(ToolCall {
        id: ToolCallId::new(id).unwrap(),
        name: ToolName::new(name).unwrap(),
        arguments,
    })
}

/// The agent's turn on `chat` awaiting the results of `calls`.
pub(crate) fn pending(calls: Vec<AssistantBlock>) -> State {
    let step = Step::new(calls, StopReason::AwaitingToolResults, None, None).unwrap();
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            InputId::new("i-go").unwrap(),
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("go").unwrap())],
        )
        .unwrap(),
    )
    .unwrap();
    chat.push_turn(Turn::new(TurnId::new("t0").unwrap(), Some(author()), step))
        .unwrap();
    let values = BTreeMap::from([
        (key("chat"), Value::conversation(chat)),
        (key("log"), Value::list(vec![Value::str("one entry")])),
        (key("enabled"), Value::list(Vec::new())),
    ]);
    State::new(schema(), values).unwrap()
}

/// A graph of one tool node running `tools`, ending `done`.
pub(crate) fn tool_graph(tools: Vec<Arc<dyn Tool>>, width: Option<usize>, enabled: bool) -> Graph {
    let node = ToolNode {
        key: key("chat"),
        author: author(),
        tools,
        max_concurrency: width.map(|width| Limit::Fixed(NonZeroUsize::new(width).unwrap())),
        enabled: enabled.then(|| key("enabled")),
        gates: Arc::new(SharedGates::default()),
        durable: None,
    };
    GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(nid("tools"), node)
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

pub(crate) async fn run_tools(graph: &Graph, state: State) -> Result<Outcome, RunFailure> {
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut channel) = control();
    run(graph, &config(), state, None, &ctx, &mut channel).await
}

/// The tool results of the agent's turn, as `(tool, is_error, text)`.
pub(crate) fn answered(result: Result<Outcome, RunFailure>) -> Vec<(String, bool, String)> {
    let Outcome::Finished { state, .. } = result.map_err(|failure| failure.error).unwrap() else {
        panic!("expected finished");
    };
    let Some(Entry::Turn(turn)) = state.conversation(&key("chat")).unwrap().entries().last() else {
        panic!("expected the agent's turn");
    };
    results_of(turn)
}

pub(crate) fn ran(name: &str) -> (String, bool, String) {
    (name.to_owned(), false, format!("{name} ran"))
}

pub(crate) fn refused(name: &str, reason: &str) -> (String, bool, String) {
    (name.to_owned(), true, reason.to_owned())
}
