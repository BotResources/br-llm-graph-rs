//! A ReAct agent that starts background work and returns at once, called
//! with `SubGraph`. After the agent, a gate ends the run `waiting` until the
//! completion arrives through the session's inbox as a runtime input; the
//! next run gives it to the agent and ends `done`.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use br_llm_graph::{
    Config, Edge, EndLabel, FnEdge, GraphBuilder, Input, Kind, NodeId, Outcome, Output, OutputMode,
    ReactAgent, Schema, Session, Source, State, SubGraph, Target, Value,
};
use br_llm_messages::{Author, Conversation, Entry, UserSource};

use common::{StartTaskTool, context, key, text_step, tool_call_step, user};
use serde_json::json;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let model = Arc::new(common::ScriptedModel::new(vec![
        tool_call_step("c1", "start_task", json!({})),
        text_step("Task started; I will wait."),
        text_step("The task is complete."),
    ]));

    let start_task = Arc::new(StartTaskTool {
        sender: Arc::new(Mutex::new(None)),
        chat: key("history"),
        pending: key("pending_tasks"),
    });

    // Inputs `history` and `pending_tasks` (written by the tool);
    // configuration `base`.
    let agent = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model,
        system: vec![Source::Config(key("base"))],
        output: OutputMode::Text,
        tools: vec![start_task],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        thinking: None,
        enabled: None,
        inputs: vec![(key("pending_tasks"), Kind::list(Kind::Str))],
    }
    .graph()
    .expect("agent graph");

    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("pending_tasks"), Kind::list(Kind::Str))
        .config(key("base"), Kind::Str)
        .build();
    let call = SubGraph::call(agent)
        .input(key("history"), Input::From(key("chat")))
        .input(key("pending_tasks"), Input::From(key("pending_tasks")))
        .config(key("base"), Input::Config(key("base")))
        .output(key("history"), Output::Set(key("chat")));
    let graph = Arc::new(
        GraphBuilder::new(schema.clone())
            .entry(NodeId::new("agent").expect("id"))
            .subgraph(NodeId::new("agent").expect("id"), call)
            .edge(NodeId::new("agent").expect("id"), gate_edge())
            .input(key("chat"))
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

    let mut session = Session::new(graph, config, state, context()).expect("session");
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
