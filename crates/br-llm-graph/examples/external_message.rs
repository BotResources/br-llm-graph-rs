//! Messages that reach a durable ReAct agent from outside: each call gives
//! them as its `opening`, in the agent's thread. A follow-up of the user,
//! then a message of an upper agent (a user input with an author of its
//! own), each answered in its turn; the model reads the whole thread, and
//! the upper agent's message framed with its author.
//!
//! The agent reads its `opening` when it enters: what a host has to say to
//! it while it runs waits for its next call.

#[allow(dead_code)]
mod common;

use std::sync::Arc;

use br_llm_clients_core::{
    BoxFuture, CallOutcome, Client, ClientRequest, ClientSettings, OutputMode, ReasoningLevel,
    StreamSink,
};
use br_llm_graph::{
    Config, Graph, Model, Outcome, Prompt, ReactAgent, StateKey, ThreadId, control, run,
};
use br_llm_messages::{
    Author, Conversation, InputId, Text, UserBlock, UserInput, UserSource, WireMessage,
    WireUserBlock,
};
use serde_json::json;

use common::store::MemoryStore;
use common::tools::EchoTool;
use common::{opening, quiet_context, scripted_at, state_key, text_step, tool_call_step, user};

/// A client that prints what each request ends with, then hands it to the
/// client it wraps: a host decorates its clients so.
struct Printing<C> {
    inner: C,
}

impl<C: Client> Client for Printing<C> {
    fn call<'a>(
        &'a self,
        request: &'a ClientRequest,
        sink: &'a mut dyn StreamSink,
    ) -> BoxFuture<'a, CallOutcome> {
        let last = request
            .messages()
            .iter()
            .rev()
            .find_map(|message| match message {
                WireMessage::User { content } => {
                    content.iter().rev().find_map(|block| match block {
                        WireUserBlock::Text { text } => Some(text.as_str().to_owned()),
                        _ => None,
                    })
                }
                WireMessage::Assistant { .. } | WireMessage::Relay { .. } => None,
            });
        println!(
            "  model: {} messages, the last user text: {:?}",
            request.messages().len(),
            last.unwrap_or_default()
        );
        self.inner.call(request, sink)
    }

    fn settings(&self) -> &ClientSettings {
        self.inner.settings()
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = Arc::new(Printing {
        inner: scripted_at(
            ReasoningLevel::Medium,
            vec![
                tool_call_step("c1", "echo", json!({})),
                text_step("Done."),
                text_step("Done, and supersteps are rounds of nodes."),
                text_step("For the board: the task is done."),
            ],
        ),
    });
    let store = Arc::new(MemoryStore::new());
    let graph = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model: Model::client(client),
        system: Prompt::text("You are an agent."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(EchoTool::new())],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        reply_retries: None,
        reminder: None,
        inputs: Vec::new(),
        thread_store: Some(store.clone()),
    }
    .graph()
    .expect("agent graph");

    println!("external_message: the task");
    let thread = call(&graph, opening([user("Do the task.")]), "").await;
    println!("external_message: a follow-up of the user");
    let thread = call(&graph, opening([user("Also mention supersteps.")]), &thread).await;
    println!("external_message: a message of an upper agent");
    let supervisor = UserInput::new(
        InputId::new("i-board").expect("input id"),
        UserSource::Human,
        Some(Author::new("supervisor", "supervisor").expect("author")),
        vec![UserBlock::text(
            Text::new("Now summarize for the board.").expect("text"),
        )],
    )
    .expect("input");
    let thread = call(&graph, opening([supervisor]), &thread).await;
    store.print(&ThreadId::new(thread).expect("a thread id"));
}

/// One call of the agent in `thread` on `opening`: the id of its thread.
async fn call(graph: &Graph, opening: Conversation, thread: &str) -> String {
    let (opening_key, thread_key, answer): (
        StateKey<Conversation>,
        StateKey<String>,
        StateKey<String>,
    ) = (
        state_key("opening"),
        state_key("thread"),
        state_key("answer"),
    );
    let state = graph
        .start_state([
            opening_key.given(opening).expect("opening"),
            thread_key.given(thread).expect("thread"),
        ])
        .expect("start state");
    let config = Config::new(graph.schema(), Default::default()).expect("config");
    let (_sender, mut channel) = control();
    match run(graph, &config, state, None, &quiet_context(), &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => {
            println!("  answer: {}", state.get(&answer).expect("answer"));
            state.get(&thread_key).expect("thread").to_owned()
        }
        Ok(_) => panic!("the call was paused or cancelled"),
        Err(failure) => panic!("the call failed: {}", failure.error),
    }
}
