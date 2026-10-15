//! A ReAct agent built as a graph and called from another graph with
//! `SubGraph`: the caller hands it a chat history, gets the history and the
//! reply back. The model is a `Client` of br-llm-clients-core; its settings,
//! the level of reasoning included, are fixed when the host builds it.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, Config, EndLabel, GraphBuilder, Input, Kind, NodeId, Outcome, Output, ReactAgent,
    Schema, Source, SubGraph, Target, Value, channel, run,
};
use br_llm_messages::{Author, Conversation};
use serde_json::json;

use common::tools::SearchTool;
use common::{context, key, scripted, text_step, tool_call_step, user};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = scripted(vec![
        tool_call_step("c1", "search", json!({ "q": "rust" })),
        text_step("Here is what I found."),
    ]);

    // Input `history`; outputs `history` and `answer`; configuration `base`
    // (the system prompt).
    let agent = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        client,
        system: vec![Source::Config(key("base"))],
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(SearchTool::new())],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
    }
    .graph()
    .expect("agent graph");

    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("answer"), Kind::Str)
        .config(key("prompt"), Kind::Str)
        .build();
    let call = SubGraph::call(agent)
        .input(key("history"), Input::From(key("chat")))
        .config(key("base"), Input::Config(key("prompt")))
        .output(key("history"), Output::Set(key("chat")))
        .output(key("answer"), Output::Set(key("answer")));
    let graph = GraphBuilder::new(schema.clone())
        .entry(NodeId::new("agent").expect("id"))
        .subgraph(NodeId::new("agent").expect("id"), call)
        .edge(
            NodeId::new("agent").expect("id"),
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .input(key("chat"))
        .build()
        .expect("build");

    let mut chat = Conversation::new();
    chat.push_input(user("Find me something about Rust."));
    let state = graph
        .start_state([(key("chat"), Value::conversation(chat))])
        .expect("start state");
    let mut values = BTreeMap::new();
    values.insert(key("prompt"), Value::str("You are a research assistant."));
    let config = Config::new(&schema, values).expect("config");

    let (_sender, mut inbox) = channel();
    match run(&graph, &config, state, None, &context(), &mut inbox).await {
        Ok(Outcome::Finished { state, end }) => {
            println!("react_agent finished on End({end})");
            println!("  answer: {}", state.str(&key("answer")).expect("answer"));
            println!(
                "  history: {} entries",
                state
                    .conversation(&key("chat"))
                    .expect("chat")
                    .entries()
                    .len()
            );
        }
        Ok(_) => println!("react_agent paused or cancelled"),
        Err(failure) => println!("react_agent failed: {}", failure.error),
    }
}
