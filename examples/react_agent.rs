#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_graph::{
    Config, EndLabel, GraphBuilder, Kind, LlmNode, NodeId, Outcome, OutputMode, ReactLoop, Schema,
    Source, State, Target, Value, channel, run,
};
use br_llm_messages::{Author, Conversation};
use serde_json::json;

use common::{
    EchoTool, ScriptedModel, SearchTool, WriteFileTool, context, key, text_step, tool_call_step,
    user,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .config(key("base"), Kind::Str)
        .build();

    let model = Arc::new(ScriptedModel::new(vec![
        tool_call_step("c1", "search", json!({ "q": "rust" })),
        text_step("Here is what I found."),
    ]));

    let llm = LlmNode {
        key: key("chat"),
        author: Author::new("agent").expect("author"),
        model,
        system: vec![Source::Config(key("base"))],
        tools: vec![
            Arc::new(SearchTool),
            Arc::new(EchoTool),
            Arc::new(WriteFileTool::default()),
        ],
        enabled: None,
        output: OutputMode::Text,
    };

    let react = ReactLoop {
        llm: NodeId::new("llm").expect("id"),
        tool_nodes: vec![
            (
                NodeId::new("safe_tools").expect("id"),
                vec![Arc::new(SearchTool), Arc::new(EchoTool)],
            ),
            (
                NodeId::new("effect_tools").expect("id"),
                vec![Arc::new(WriteFileTool::default())],
            ),
        ],
        after: Target::End(EndLabel::new("done").expect("label")),
    };

    let graph = react
        .add(
            GraphBuilder::new(schema.clone()).entry(NodeId::new("llm").expect("id")),
            llm,
        )
        .expect("partition")
        .build()
        .expect("build");

    let (_sender, mut inbox) = channel();
    let outcome = run(
        &graph,
        &config(&schema),
        state(&schema),
        None,
        &context(),
        &mut inbox,
    )
    .await;

    match outcome {
        Ok(Outcome::Finished { end, .. }) => println!("react_agent finished on End({end})"),
        Ok(_) => println!("react_agent paused or cancelled"),
        Err(failure) => println!("react_agent failed: {}", failure.error),
    }
}

fn config(schema: &Schema) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a research assistant."));
    Config::new(schema, values).expect("config")
}

fn state(schema: &Schema) -> State {
    let mut chat = Conversation::new();
    chat.push_input(user("Find me something about Rust."));
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    State::new(schema.clone(), values).expect("state")
}
