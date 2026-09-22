use std::sync::Arc;

use crate::graph::{Always, Context, GraphBuilder, Target};
use crate::observe::NoopObserver;
use crate::react::llm_node::{LlmNode, Source};
use crate::react::model::OutputMode;
use crate::react::test_support::*;
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

#[tokio::test]
async fn given_enabled_filter_when_llm_runs_then_only_enabled_tools_declared() {
    use crate::state::Value;
    let model = Arc::new(RecordingModel::new());
    let recording = model.clone();
    let llm = LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base"))],
        tools: vec![Arc::new(EchoTool), Arc::new(TallyTool)],
        enabled: Some(key("enabled")),
        output: OutputMode::Text,
    };
    let graph = GraphBuilder::new(schema())
        .entry(nid("llm"))
        .node(nid("llm"), llm)
        .edge(
            nid("llm"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();

    let mut state = seeded_state();
    state
        .apply_batch(&[crate::update::Update::Set {
            key: key("enabled"),
            value: Value::list(vec![Value::str("echo")]),
        }])
        .unwrap();
    let (_sender, mut inbox) = channel();
    run(&graph, &config(), state, None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let seen = recording.tools_seen.lock().unwrap().clone();
    assert_eq!(seen, vec!["echo".to_owned()]);
}

#[tokio::test]
async fn given_multiple_system_sources_when_llm_runs_then_joined_by_blank_line() {
    let model = Arc::new(RecordingModel::new());
    let recording = model.clone();
    let llm = LlmNode {
        key: key("chat"),
        author: author(),
        model,
        system: vec![Source::Config(key("base")), Source::State(key("enabled"))],
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Text,
    };
    let graph = GraphBuilder::new(schema())
        .entry(nid("llm"))
        .node(nid("llm"), llm)
        .edge(
            nid("llm"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let (_sender, mut inbox) = channel();
    run(&graph, &config(), seeded_state(), None, &ctx(), &mut inbox)
        .await
        .map_err(|f| f.error)
        .unwrap();
    let system = recording.system_seen.lock().unwrap().clone().unwrap();
    assert_eq!(system, "You are a helpful agent.\n\necho\n\ntally");
}
