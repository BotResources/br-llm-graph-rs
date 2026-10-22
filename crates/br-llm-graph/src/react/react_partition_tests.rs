//! The split of the tools between the tool nodes of a ReAct loop: each
//! declared tool run by exactly one tool node, and no tool node running a
//! tool the model call does not declare.

use std::sync::Arc;

use crate::graph::{GraphBuilder, Target};
use crate::react::given::Model;
use crate::react::llm_node::LlmNode;
use crate::react::react_agent::ReactAgent;
use crate::react::react_loop::ReactLoop;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::value::{EndLabel, NodeId};
use br_llm_clients_core::OutputMode;

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
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
        tool_concurrency: None,
        round_limit: None,
        call: in_memory_call(),
        seat: None,
    }
}

#[test]
fn given_tool_not_covered_when_react_loop_added_then_refused() {
    let client = Arc::new(mock(vec![text_step("x")]));
    let llm = llm_node(client, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), Vec::new())],
        after: Target::End(EndLabel::new("done").unwrap()),
        thread: None,
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolNotCovered { .. })
    ));
}

#[test]
fn given_zero_tool_nodes_when_react_loop_added_then_tool_not_covered() {
    let client = Arc::new(mock(vec![text_step("x")]));
    let llm = llm_node(client, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: Vec::new(),
        after: Target::End(EndLabel::new("done").unwrap()),
        thread: None,
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolNotCovered { .. })
    ));
}

#[test]
fn given_tool_covered_twice_when_react_loop_added_then_refused() {
    let client = Arc::new(mock(vec![text_step("x")]));
    let llm = llm_node(client, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![
            (nid("t1"), vec![Arc::new(EchoTool)]),
            (nid("t2"), vec![Arc::new(EchoTool)]),
        ],
        after: Target::End(EndLabel::new("done").unwrap()),
        thread: None,
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolCoveredTwice { .. })
    ));
}

#[test]
fn given_tool_node_tool_not_declared_when_react_loop_added_then_refused() {
    let client = Arc::new(mock(vec![text_step("x")]));
    let llm = llm_node(client, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool), Arc::new(TallyTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
        thread: None,
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolNotDeclared { .. })
    ));
}

#[test]
fn given_one_tool_node_listing_a_tool_twice_when_react_loop_added_then_tool_covered_twice_saying_so()
 {
    let llm = llm_node(Arc::new(mock(Vec::new())), vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool), Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
        thread: None,
    };
    let error = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .err()
        .unwrap();
    assert!(matches!(
        error,
        crate::error::GraphError::ToolCoveredTwice { .. }
    ));
    assert_eq!(
        error.to_string(),
        "tool echo is run more than once by the tool nodes (twice in one node or in two nodes)"
    );
}
