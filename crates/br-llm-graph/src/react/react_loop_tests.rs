use std::sync::Arc;

use serde_json::json;

use crate::graph::{Context, GraphBuilder, Target};
use crate::observe::NoopObserver;
use crate::react::given::Model;
use crate::react::llm_node::LlmNode;
use crate::react::react_agent::ReactAgent;
use crate::react::react_loop::ReactLoop;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::run::Outcome;
use crate::run::control;
use crate::run::run;
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};
use br_llm_clients_core::OutputMode;

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

fn llm_node(
    client: Arc<dyn br_llm_clients_core::Client>,
    tools: Vec<Arc<dyn Tool>>,
) -> LlmNode<ReactAgent> {
    LlmNode::<ReactAgent> {
        key: key("chat"),
        author: author(),
        model: Model::client(client),
        system: base(),
        history: None,
        tools,
        enabled: None,
        output_mode: OutputMode::Plain,
        call: None,
        seat: None,
    }
}

#[tokio::test]
async fn given_a_full_react_loop_when_run_then_finishes_with_turn_of_three_items() {
    let client = Arc::new(mock(vec![
        tool_call_step("c1", "echo", json!({})),
        text_step("all done"),
    ]));
    let llm = llm_node(client, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
        tool_concurrency: None,
        round_limit: None,
        thread: None,
    };
    let graph = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap();

    let (_sender, mut channel) = control();
    let outcome = run(
        &graph,
        &config(),
        seeded_state(),
        None,
        &ctx(),
        &mut channel,
    )
    .await
    .map_err(|f| f.error)
    .unwrap();
    let Outcome::Finished { state, end } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(end.as_str(), "done");
    let convo = state.conversation(&key("chat")).unwrap();
    let turn = convo
        .entries()
        .iter()
        .find_map(|entry| match entry {
            br_llm_messages::Entry::Turn(turn) => Some(turn),
            br_llm_messages::Entry::UserInput(_) => None,
        })
        .unwrap();
    assert_eq!(turn.items().len(), 3);
}

#[tokio::test]
async fn given_two_tool_nodes_with_parallel_calls_when_run_then_both_run() {
    let client = Arc::new(mock(vec![two_call_step(), text_step("done both")]));
    let llm = llm_node(client, vec![Arc::new(EchoTool), Arc::new(TallyTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![
            (nid("echo_node"), vec![Arc::new(EchoTool)]),
            (nid("tally_node"), vec![Arc::new(TallyTool)]),
        ],
        after: Target::End(EndLabel::new("done").unwrap()),
        tool_concurrency: None,
        round_limit: None,
        thread: None,
    };
    let graph = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap();
    let (_sender, mut channel) = control();
    let outcome = run(
        &graph,
        &config(),
        seeded_state(),
        None,
        &ctx(),
        &mut channel,
    )
    .await
    .map_err(|f| f.error)
    .unwrap();
    let Outcome::Finished { state, .. } = outcome else {
        panic!("expected finished");
    };
    assert_eq!(state.list(&key("log")).unwrap().len(), 1);
}

fn two_call_step() -> br_llm_messages::Step {
    use br_llm_messages::{AssistantBlock, Step, StopReason, ToolCall, ToolCallId, ToolName};
    Step::new(
        vec![
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c1").unwrap(),
                name: ToolName::new("echo").unwrap(),
                arguments: json!({}),
            }),
            AssistantBlock::ToolCall(ToolCall {
                id: ToolCallId::new("c2").unwrap(),
                name: ToolName::new("tally").unwrap(),
                arguments: json!({}),
            }),
        ],
        StopReason::AwaitingToolResults,
        None,
        None,
    )
    .unwrap()
}
