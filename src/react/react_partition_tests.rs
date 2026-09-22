use std::sync::Arc;

use serde_json::json;

use crate::graph::{Context, GraphBuilder, Target};
use crate::observe::NoopObserver;
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::OutputMode;
use crate::react::react_loop::ReactLoop;
use crate::react::test_support::*;
use crate::react::tool::Tool;
use crate::run::channel;
use crate::run::run;
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

fn llm_node(model: Arc<dyn crate::react::model::Model>, tools: Vec<Arc<dyn Tool>>) -> LlmNode {
    LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base"))],
        tools,
        enabled: None,
        output: OutputMode::Text,
    }
}

#[test]
fn given_tool_not_covered_when_react_loop_added_then_refused() {
    let model = Arc::new(ScriptedModel::new(vec![text_step("x")]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), Vec::new())],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolNotCovered { .. })
    ));
}

#[test]
fn given_tool_covered_twice_when_react_loop_added_then_refused() {
    let model = Arc::new(ScriptedModel::new(vec![text_step("x")]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![
            (nid("t1"), vec![Arc::new(EchoTool)]),
            (nid("t2"), vec![Arc::new(EchoTool)]),
        ],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolCoveredTwice { .. })
    ));
}

#[test]
fn given_tool_node_tool_not_declared_when_react_loop_added_then_refused() {
    let model = Arc::new(ScriptedModel::new(vec![text_step("x")]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool), Arc::new(TallyTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let result = react.add(GraphBuilder::new(schema()).entry(nid("llm")), llm);
    assert!(matches!(
        result.err(),
        Some(crate::error::GraphError::ToolNotDeclared { .. })
    ));
}

#[tokio::test]
async fn given_model_calls_unowned_tool_when_run_then_pending_unsatisfiable() {
    let model = Arc::new(ScriptedModel::new(vec![tool_call_step(
        "c1",
        "ghost",
        json!({}),
    )]));
    let llm = llm_node(model, vec![Arc::new(EchoTool)]);
    let react = ReactLoop {
        llm: nid("llm"),
        tool_nodes: vec![(nid("tools"), vec![Arc::new(EchoTool)])],
        after: Target::End(EndLabel::new("done").unwrap()),
    };
    let graph = react
        .add(GraphBuilder::new(schema()).entry(nid("llm")), llm)
        .unwrap()
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    let failure = run(&graph, &config(), seeded_state(), None, &ctx(), &mut inbox)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        crate::error::GraphError::PendingToolUnsatisfiable { .. }
    ));
}
