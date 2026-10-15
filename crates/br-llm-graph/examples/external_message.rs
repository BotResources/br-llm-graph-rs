//! Messages that reach a ReAct agent from outside its run: a user note sent
//! through the inbox while a tool call is open, then a message of an upper
//! agent added to the history the host saved before running the agent again.

#[allow(dead_code)]
mod common;

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use br_llm_graph::{
    Config, Model, ModelError, ModelFuture, Outcome, OutputMode, ReactAgent, Request, Source,
    StreamSink, Update, Value, channel, run,
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
    let model: Arc<dyn Model> = Arc::new(PrintingModel::new(
        "model",
        vec![
            tool_call_step("c1", "echo", json!({})),
            text_step("Handled, and I saw your note."),
            text_step("Handled the supervisor request too."),
        ],
    ));

    // Input `history`; configuration `base` (the system prompt).
    let graph = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model,
        system: vec![Source::Config(key("base"))],
        output: OutputMode::Text,
        tools: vec![Arc::new(EchoTool)],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        thinking: None,
        enabled: None,
        inputs: Vec::new(),
    }
    .graph()
    .expect("agent graph");

    let mut config_values = BTreeMap::new();
    config_values.insert(key("base"), Value::str("You are an agent."));
    let config = Config::new(graph.schema(), config_values).expect("config");

    let mut chat = Conversation::new();
    chat.push_input(user("Do the task."));
    let first_state = graph
        .start_state([(key("history"), Value::conversation(chat))])
        .expect("start state");

    let ctx = context();

    println!("external_message: first run (a user note arrives while the tool call is open)");
    let (sender, mut inbox) = channel();
    sender.send(key("history"), user("Actually, also mention supersteps."));
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
        Some(Author::new("supervisor", "supervisor").expect("author")),
        vec![UserBlock::text(
            Text::new("Now summarize for the board.").expect("text"),
        )],
    )
    .expect("input");
    relaunch_state
        .apply_batch(&[Update::Input {
            key: key("history"),
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
