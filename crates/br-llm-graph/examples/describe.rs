//! A built graph describes itself as data, so a host or a CI job can compare
//! the graph it built with the graph it meant to build (a Runner's intent
//! graph). This graph plans topics, summarizes each through a call of a
//! one-node graph (a map), then asks a ReAct agent; its edges declare their
//! targets and two nodes carry realizes labels. The example prints the
//! description as JSON: nothing runs, no client is called.

#[allow(dead_code)]
mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, Config, Context, EndLabel, FnEdge, FnNode, Graph, GraphBuilder, Input, ItemFailure,
    Limit, LlmCall, LlmRequest, Map, Model, Node, NodeFuture, NodeId, OnLimit, Output, Prompt,
    ReactAgent, RoundLimit, Schema, State, StateKey, SubGraph, Target, TextOutput,
};
use br_llm_messages::{Author, Conversation};

use common::tools::{EchoTool, SearchTool};
use common::{key, scripted};

fn id(name: &str) -> NodeId {
    NodeId::new(name).expect("valid node id")
}

fn end(label: &str) -> Target {
    Target::End(EndLabel::new(label).expect("valid label"))
}

/// One line about a topic: a graph of one LLM call.
fn summarize() -> Arc<Graph> {
    let mut schema = Schema::builder();
    let topic: StateKey<String> = schema.state("topic").expect("key");
    let note: StateKey<String> = schema.state("note").expect("key");
    let call = LlmCall {
        model: Model::client(scripted(Vec::new())),
        system: Prompt::text("Summarize the topic in one line."),
        request: LlmRequest::Text(Prompt::from_state(&topic)),
        output_mode: OutputMode::Plain,
        output: TextOutput::Set(note.clone()),
        reply_retries: None,
        reminder: None,
    };
    let graph = GraphBuilder::new(schema.build())
        .entry(id("summarize"))
        .node(id("summarize"), call)
        .edge(id("summarize"), Always(end("done")))
        .input(&topic)
        .output(&note)
        .build()
        .expect("the summary graph builds");
    Arc::new(graph)
}

/// The researcher: a ReAct agent with two tools and a bounded number of
/// tool rounds.
fn researcher() -> Arc<Graph> {
    ReactAgent {
        author: Author::new("researcher", "Researcher").expect("author"),
        model: Model::client(scripted(Vec::new())),
        system: Prompt::text("Answer the question from what you find."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(SearchTool::new()), Arc::new(EchoTool::new())],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(4).expect("positive")),
            on_limit: OnLimit::Error,
        }),
        enabled: None,
        reply_retries: None,
        reminder: None,
        inputs: Vec::new(),
        thread_store: None,
    }
    .graph()
    .expect("the researcher builds")
}

fn main() {
    let mut schema = Schema::builder();
    let topics: StateKey<Vec<String>> = schema.state("topics").expect("key");
    let topic: StateKey<String> = schema.state("topic").expect("key");
    let notes: StateKey<Vec<String>> = schema.state("notes").expect("key");
    let question: StateKey<Conversation> = schema.state("question").expect("key");
    let report: StateKey<String> = schema.state("report").expect("key");
    let width = schema.config::<i64>("width").expect("key");

    let plan = FnNode::new(|_: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
        Box::pin(async { Ok(Vec::new()) })
    });
    let body = SubGraph::call(summarize())
        .input(topic.key().clone(), Input::From(topic.key().clone()))
        .output(key("note"), Output::Append(notes.key().clone()));
    let summaries = Map::new(&topics, &topic, body)
        .max_concurrency(Limit::config(&width))
        .on_item_failure(ItemFailure::FailFast);
    // Declared: the description lists both targets, and the run refuses
    // any other (`UndeclaredTarget`).
    let read = notes.clone();
    let after = FnEdge::new(move |state: &State, _: &Config| {
        Ok(vec![if state.get(&read)?.is_empty() {
            end("skipped")
        } else {
            Target::Node(id("researcher"))
        }])
    })
    .targets([Target::Node(id("researcher")), end("skipped")]);
    let ask = SubGraph::call(researcher())
        .input(key("opening"), Input::From(question.key().clone()))
        .output(key("answer"), Output::Set(report.key().clone()));

    let graph = GraphBuilder::new(schema.build())
        .entry(id("plan"))
        .node(id("plan"), plan.realizing(["PlanTopics"]))
        .map(id("summaries"), summaries)
        .node(id("researcher"), ask.realizing(["Researcher"]))
        .edge(id("plan"), Always(Target::Node(id("summaries"))))
        .edge(id("summaries"), after)
        .edge(id("researcher"), Always(end("done")))
        .input(&question)
        .input(&topics)
        .output(&notes)
        .output(&report)
        .build()
        .expect("the graph builds");

    let description = graph.describe();
    println!(
        "{}",
        serde_json::to_string_pretty(&description).expect("a description is written as JSON")
    );
}
