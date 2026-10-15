//! Messages that reach a ReAct agent from outside its run: a user note sent
//! through the control channel while a tool call is open, then a message of
//! an upper agent added to the history the host saved before running the
//! agent again.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{
    BoxFuture, CallOutcome, Client, ClientRequest, ClientSettings, OutputMode, ReasoningLevel,
    StreamSink,
};
use br_llm_graph::{Config, Outcome, ReactAgent, Source, Update, Value, control, run};
use br_llm_messages::{
    Author, Conversation, InputId, Text, UserBlock, UserInput, UserSource, WireMessage,
};
use serde_json::json;

use common::tools::EchoTool;
use common::{context, key, scripted_at, text_step, tool_call_step, user};

/// A client that prints the roles of each request it receives, then hands
/// the request to the client it wraps: a host decorates its clients so.
struct Printing<C> {
    label: String,
    inner: C,
}

impl<C: Client> Client for Printing<C> {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        println!("  [{}] request roles:", self.label);
        for message in request.messages() {
            let role = match message {
                WireMessage::User { .. } => "user",
                WireMessage::Assistant { .. } => "assistant",
                WireMessage::Relay { .. } => "relay",
            };
            println!("    - {role}");
        }
        self.inner.call(request, sink)
    }

    fn settings(&self) -> &ClientSettings {
        self.inner.settings()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = Arc::new(Printing {
        label: "model".to_owned(),
        inner: scripted_at(
            ReasoningLevel::Medium,
            vec![
                tool_call_step("c1", "echo", json!({})),
                text_step("Handled, and I saw your note."),
                text_step("Handled the supervisor request too."),
            ],
        ),
    });

    // Input `history`; configuration `base` (the system prompt).
    let graph = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        client,
        system: vec![Source::Config(key("base"))],
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(EchoTool::new())],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
    }
    .graph()
    .expect("agent graph");

    let mut config_values = BTreeMap::new();
    config_values.insert(key("base"), Value::str("You are an agent."));
    let config = Config::new(graph.schema(), config_values).expect("config");

    let mut chat = Conversation::new();
    chat.push_input(user("Do the task."))
        .expect("an input id unique in the conversation");
    let first_state = graph
        .start_state([(key("history"), Value::conversation(chat))])
        .expect("start state");

    let ctx = context();

    println!("external_message: first run (a user note arrives while the tool call is open)");
    let (sender, mut channel) = control();
    sender.send(key("history"), user("Actually, also mention supersteps."));
    let outcome = run(&graph, &config, first_state, None, &ctx, &mut channel).await;
    let Ok(Outcome::Finished { state, end }) = outcome else {
        println!("external_message: first run did not finish cleanly");
        return;
    };
    println!("external_message: first run ended on End({end})");

    println!("external_message: second run (an upper agent relaunches with its own message)");
    let mut relaunch_state = state;
    let supervisor = UserInput::new(
        InputId::new("i-now-summarize-for-the-board").expect("input id"),
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
    let (_sender, mut channel) = control();
    let outcome = run(&graph, &config, relaunch_state, None, &ctx, &mut channel).await;
    match outcome {
        Ok(Outcome::Finished { end, .. }) => {
            println!("external_message: second run ended on End({end})")
        }
        Ok(_) => println!("external_message: second run paused or cancelled"),
        Err(failure) => println!("external_message: second run failed: {}", failure.error),
    }
}
