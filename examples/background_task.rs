#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use br_llm_graph::{
    Config, Context, Edge, EndLabel, FnEdge, FnNode, GraphBuilder, Kind, LlmNode, NodeFuture,
    NodeId, Outcome, OutputMode, ReactLoop, Schema, Session, Source, State, Target, Value,
};
use br_llm_messages::{Author, Conversation, Entry, UserSource};

use common::{StartTaskTool, context, key, text_step, tool_call_step, user};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("pending_tasks"), Kind::list(Kind::Str))
        .config(key("base"), Kind::Str)
        .build();

    let model = Arc::new(common::ScriptedModel::new(vec![
        tool_call_step("c1", "start_task", json!({})),
        text_step("Task started; I will wait."),
        text_step("The task is complete."),
    ]));

    let start_task = Arc::new(StartTaskTool {
        sender: Arc::new(Mutex::new(None)),
        chat: key("chat"),
        pending: key("pending_tasks"),
    });

    let llm = LlmNode {
        key: key("chat"),
        author: Author::new("agent").expect("author"),
        model,
        system: vec![Source::Config(key("base"))],
        tools: vec![start_task.clone()],
        enabled: None,
        output: OutputMode::Text,
    };
    let react = ReactLoop {
        llm: NodeId::new("llm").expect("id"),
        tool_nodes: vec![(NodeId::new("tools").expect("id"), vec![start_task])],
        after: Target::Node(NodeId::new("gate").expect("id")),
    };
    let graph = Arc::new(
        react
            .add(
                GraphBuilder::new(schema.clone()).entry(NodeId::new("llm").expect("id")),
                llm,
            )
            .expect("partition")
            .node(NodeId::new("gate").expect("id"), noop())
            .edge(NodeId::new("gate").expect("id"), gate_edge())
            .build()
            .expect("build"),
    );

    let mut config_values = BTreeMap::new();
    config_values.insert(
        key("base"),
        Value::str("Kick off background work, then wait."),
    );
    let config = Config::new(&schema, config_values).expect("config");

    let mut chat = Conversation::new();
    chat.push_input(user("Please run the long job."));
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("pending_tasks"), Value::list(Vec::new()));
    let state = State::new(schema.clone(), values).expect("state");

    let mut session = Session::new(graph, config, state, context());
    let sender = session.sender();

    let first = session.run_once().await.expect("run");
    report("first run", &first);

    sender.send(
        key("chat"),
        common::runtime_input("task_done", "task-1 finished"),
    );
    let second = session.run_once().await.expect("run");
    report("second run", &second);
}

fn report(label: &str, outcome: &Outcome) {
    match outcome {
        Outcome::Finished { end, .. } => println!("background_task {label}: End({end})"),
        Outcome::Paused { .. } => println!("background_task {label}: paused"),
        Outcome::Cancelled { .. } => println!("background_task {label}: cancelled"),
    }
}

fn noop() -> impl br_llm_graph::Node {
    FnNode::new(|_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
        Box::pin(async { Ok(Vec::new()) })
    })
}

fn gate_edge() -> impl Edge {
    FnEdge::new(|state: &State, _config: &Config| {
        let conversation = state.conversation(&key("chat"))?;
        let completed = conversation.entries().iter().any(|entry| {
            matches!(
                entry,
                Entry::UserInput(input) if matches!(input.source(), UserSource::Runtime { .. })
            )
        });
        if completed {
            Ok(vec![Target::End(EndLabel::new("done").expect("label"))])
        } else {
            Ok(vec![Target::End(EndLabel::new("waiting").expect("label"))])
        }
    })
}
