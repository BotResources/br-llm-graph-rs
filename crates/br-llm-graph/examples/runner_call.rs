//! Calling other runners, as nodes of a graph and as a tool of an agent,
//! through the host's runner port (here one kept in memory,
//! `common::runners`). The library gives each call its key, the origin of
//! the node that makes it and, for a tool, the id of the tool call, so a
//! resumed run finds the calls it had started; the port does the rest.
//!
//! First a map asks `ask_a_document` about each document, at most two at
//! once: the answers and the children's threads are appended in document
//! order, and a document whose runner fails is kept as data (`Capture`).
//! Then a ReAct agent offers `documentalist` as a tool: the model calls it,
//! reads its structured answer and replies.

#[allow(dead_code)]
mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, AnswerBlock, CaptureUpdate, Config, EndLabel, GraphBuilder, ItemFailure, Limit, Map,
    Model, NodeId, Outcome, Prompt, ReactAgent, RunnerArguments, RunnerCall, RunnerOutput,
    RunnerTarget, RunnerTool, Schema, StateKey, Target, control, run,
};
use br_llm_messages::{Author, Conversation, Text};
use serde_json::{Map as Object, Value as Json, json};

use common::runners::MemoryRunners;
use common::tools::{text_parameter, tool_spec};
use common::{opening, quiet_context, scripted, state_key, text_step, tool_call_step, user};

/// The runners this host reaches: `ask_a_document` answers about a document
/// it holds, `documentalist` selects the documents that can answer.
fn runners() -> Arc<MemoryRunners> {
    let text = |body: &str| AnswerBlock::Text(Text::new(body).expect("non-empty"));
    Arc::new(
        MemoryRunners::new()
            .runner("ask_a_document", move |arguments| {
                match arguments.get("document").and_then(Json::as_str) {
                    Some("tides.md") => Ok(vec![text("The moon pulls the sea.")]),
                    Some("moon.md") => Ok(vec![text("The moon turns in 27 days.")]),
                    Some(other) => Err(format!("{other} is not in the library")),
                    None => Err("no document was asked about".to_owned()),
                }
            })
            .runner("documentalist", |_| {
                Ok(vec![AnswerBlock::Structured(
                    json!({"document_ids": ["tides.md"]}),
                )])
            }),
    )
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let port = runners();

    // As nodes: one call of `ask_a_document` per document.
    let mut schema = Schema::builder();
    let documents: StateKey<Vec<String>> = schema.state("documents").expect("key");
    let document: StateKey<String> = schema.state("document").expect("key");
    let answers: StateKey<Vec<String>> = schema.state("answers").expect("key");
    let threads: StateKey<Vec<String>> = schema.state("threads").expect("key");
    let failures: StateKey<Vec<String>> = schema.state("failures").expect("key");
    let schema = schema.build();

    let asked = document.clone();
    let ask = RunnerCall {
        target: RunnerTarget::new("ask_a_document").expect("target"),
        arguments: RunnerArguments::from_fn(move |state, _config| {
            Ok(Object::from_iter([(
                "document".to_owned(),
                Json::from(state.get(&asked)?),
            )]))
        }),
        port: port.clone(),
        output: RunnerOutput::Append(answers.clone()),
        thread: Some(RunnerOutput::Append(threads.clone())),
    };
    let each = Map::new(&documents, &document, ask)
        .max_concurrency(Limit::Fixed(NonZeroUsize::new(2).expect("two")))
        .on_item_failure(ItemFailure::Capture(vec![CaptureUpdate::append_reason(
            &failures,
        )]));
    let id = |name: &str| NodeId::new(name).expect("node id");
    let graph = GraphBuilder::new(schema.clone())
        .entry(id("ask"))
        .map(id("ask"), each)
        .edge(
            id("ask"),
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .input(&documents)
        .output(&answers)
        .output(&threads)
        .output(&failures)
        .build()
        .expect("build");
    let state = graph
        .start_state([documents
            .given(
                ["tides.md", "moon.md", "comets.md"]
                    .map(str::to_owned)
                    .to_vec(),
            )
            .expect("documents")])
        .expect("start state");
    let config = Config::new(&schema, Default::default()).expect("config");
    let (_sender, mut channel) = control();
    match run(&graph, &config, state, None, &quiet_context(), &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => {
            let (answers, threads) = (
                state.get(&answers).expect("answers"),
                state.get(&threads).expect("threads"),
            );
            for (answer, thread) in answers.iter().zip(&threads) {
                println!("runner_call: {answer} (thread {thread})");
            }
            for failure in state.get(&failures).expect("failures") {
                println!("runner_call: kept as data: {failure}");
            }
        }
        Ok(_) => println!("runner_call paused or cancelled"),
        Err(failure) => println!("runner_call failed: {}", failure.error),
    }

    // As a tool: the model calls `documentalist` and reads its answer.
    let client = scripted(vec![
        tool_call_step(
            "c1",
            "select_documents",
            json!({"question": "What do tides follow?"}),
        ),
        text_step("tides.md answers: the tides follow the moon."),
    ]);
    let select = RunnerTool {
        spec: tool_spec(
            "select_documents",
            "choose the documents that can answer a question",
            vec![text_parameter("question", "the question to answer")],
        ),
        target: RunnerTarget::new("documentalist").expect("target"),
        port: port.clone(),
        hook: None,
    };
    let agent = ReactAgent {
        author: Author::new("librarian", "librarian").expect("author"),
        model: Model::client(client),
        system: Prompt::text("Answer from the library's documents."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(select)],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        reply_retries: None,
        reminder: None,
        inputs: Vec::new(),
        thread_store: None,
    }
    .graph()
    .expect("agent graph");
    let answer: StateKey<String> = state_key("answer");
    let state = agent
        .start_state([state_key::<Conversation>("opening")
            .given(opening([user("What do tides follow?")]))
            .expect("opening")])
        .expect("start state");
    let config = Config::new(agent.schema(), Default::default()).expect("config");
    let (_sender, mut channel) = control();
    match run(&agent, &config, state, None, &quiet_context(), &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => println!(
            "runner_call: the agent says {} ({} calls settled, {} children cancelled)",
            state.get(&answer).expect("answer"),
            port.settled(),
            port.cancelled_children().len()
        ),
        Ok(_) => println!("runner_call agent paused or cancelled"),
        Err(failure) => println!("runner_call agent failed: {}", failure.error),
    }
}
