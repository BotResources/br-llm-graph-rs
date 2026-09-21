#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use br_llm_graph::{
    Config, EndLabel, GraphBuilder, Kind, LlmNode, Model, ModelError, ModelFuture, NodeId, Outcome,
    OutputMode, ReactLoop, Request, Schema, Source, State, StreamSink, Target, Update, Value,
    channel, run,
};
use br_llm_messages::{
    Author, Conversation, Step, Text, UserBlock, UserInput, UserSource, WireMessage,
};
use serde_json::json;

use common::{EchoTool, context, key, text_step, tool_call_step, user};

struct PrintingModel {
    label: String,
    steps: Mutex<VecDeque<Step>>,
}

impl PrintingModel {
    fn new(label: &str, steps: Vec<Step>) -> Self {
        Self {
            label: label.to_owned(),
            steps: Mutex::new(steps.into_iter().collect()),
        }
    }
}

impl Model for PrintingModel {
    fn complete<'a>(&'a self, request: Request, _sink: &'a dyn StreamSink) -> ModelFuture<'a> {
        println!("  [{}] request roles:", self.label);
        for message in &request.messages {
            let role = match message {
                WireMessage::User { .. } => "user",
                WireMessage::Assistant { .. } => "assistant",
                WireMessage::Relay { .. } => "relay",
            };
            println!("    - {role}");
        }
        let step = self
            .steps
            .lock()
            .ok()
            .and_then(|mut queue| queue.pop_front());
        Box::pin(
            async move { step.ok_or_else(|| -> ModelError { "printing model exhausted".into() }) },
        )
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .config(key("base"), Kind::Str)
        .build();

    let model: Arc<dyn Model> = Arc::new(PrintingModel::new(
        "model",
        vec![
            tool_call_step("c1", "echo", json!({})),
            text_step("Handled, and I saw your note."),
            text_step("Handled the supervisor request too."),
        ],
    ));

    let llm = LlmNode {
        key: key("chat"),
        author: Author::new("agent").expect("author"),
        model,
        system: vec![Source::Config(key("base"))],
        tools: vec![Arc::new(EchoTool)],
        enabled: None,
        output: OutputMode::Text,
    };
    let react = ReactLoop {
        llm: NodeId::new("llm").expect("id"),
        tool_nodes: vec![(NodeId::new("tools").expect("id"), vec![Arc::new(EchoTool)])],
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

    let mut config_values = BTreeMap::new();
    config_values.insert(key("base"), Value::str("You are an agent."));
    let config = Config::new(&schema, config_values).expect("config");

    let mut chat = Conversation::new();
    chat.push_input(user("Do the task."));
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    let first_state = State::new(schema.clone(), values).expect("state");

    let ctx = context();

    println!("external_message: first run (a user note arrives while the tool call is open)");
    let (sender, mut inbox) = channel();
    sender.send(key("chat"), user("Actually, also mention supersteps."));
    let outcome = run(&graph, &config, first_state, None, &ctx, &mut inbox).await;
    let Ok(Outcome::Finished { state, end }) = outcome else {
        println!("external_message: first run did not finish cleanly");
        return;
    };
    println!("external_message: first run ended on End({end})");

    println!("external_message: second run (an upper agent relaunches with its own message)");
    let mut relaunch_state = state;
    let supervisor = UserInput::new(
        UserSource::Human,
        Some(Author::new("supervisor").expect("author")),
        vec![UserBlock::text(
            Text::new("Now summarize for the board.").expect("text"),
        )],
    )
    .expect("input");
    relaunch_state
        .apply_batch(&[Update::Input {
            key: key("chat"),
            input: supervisor,
        }])
        .expect("apply");
    let (_sender, mut inbox) = channel();
    let outcome = run(&graph, &config, relaunch_state, None, &ctx, &mut inbox).await;
    match outcome {
        Ok(Outcome::Finished { end, .. }) => {
            println!("external_message: second run ended on End({end})")
        }
        Ok(_) => println!("external_message: second run paused or cancelled"),
        Err(failure) => println!("external_message: second run failed: {}", failure.error),
    }
}
