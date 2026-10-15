//! One model call per item, then one over the results: `LlmCall` as the body
//! of a `Map` appends a summary per document, in document order, at most two
//! calls at once; a second `LlmCall` reads the summaries as its request and
//! sets a title. Each call keeps no conversation: its answer is data.

#[allow(dead_code)]
mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, Config, EndLabel, GraphBuilder, ItemFailure, Kind, Limit, LlmCall, LlmRequest, Map,
    NodeId, Outcome, Output, Schema, Source, Target, Value, channel, run,
};

use common::{context, key, scripted, text_step};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = scripted(vec![
        text_step("Tides follow the moon."),
        text_step("Spring tides come with full and new moons."),
        text_step("Neap tides come with quarter moons."),
        text_step("How the moon drives the tides"),
    ]);

    // State: the documents, the one being read, the summaries and the title;
    // configuration: the style every call is asked to keep.
    let schema = Schema::builder()
        .state(key("documents"), Kind::list(Kind::Str))
        .state(key("document"), Kind::Str)
        .state(key("summaries"), Kind::list(Kind::Str))
        .state(key("title"), Kind::Str)
        .config(key("style"), Kind::Str)
        .build();

    let summarize = LlmCall {
        client: client.clone(),
        system: Source::Config(key("style")),
        request: LlmRequest::Text(Source::Fn(Arc::new(|state, _config| {
            Ok(format!(
                "Summarize in one sentence:\n{}",
                state.str(&key("document"))?
            ))
        }))),
        output_mode: OutputMode::Plain,
        output: Output::Append(key("summaries")),
    };
    let each = Map {
        list: key("documents"),
        item: key("document"),
        body: Box::new(summarize),
        max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(2).expect("two"))),
        on_item_failure: ItemFailure::Finish,
    };
    // A list of strings is a source too: its items joined by a blank line.
    let entitle = LlmCall {
        client,
        system: Source::Config(key("style")),
        request: LlmRequest::Text(Source::State(key("summaries"))),
        output_mode: OutputMode::Plain,
        output: Output::Set(key("title")),
    };

    let id = |name: &str| NodeId::new(name).expect("node id");
    let graph = GraphBuilder::new(schema.clone())
        .entry(id("summarize"))
        .map(id("summarize"), each)
        .edge(id("summarize"), Always(Target::Node(id("entitle"))))
        .node(id("entitle"), entitle)
        .edge(
            id("entitle"),
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .input(key("documents"))
        .output(key("summaries"))
        .output(key("title"))
        .build()
        .expect("build");

    let documents = [
        "The tides rise and fall twice a day, pulled by the moon.",
        "At full and new moon, sun and moon pull together: spring tides.",
        "At the quarter moons they pull apart: neap tides.",
    ];
    let state = graph
        .start_state([(
            key("documents"),
            Value::list(documents.into_iter().map(Value::str).collect()),
        )])
        .expect("start state");
    let config = Config::new(
        &schema,
        [(key("style"), Value::str("Write plain English."))].into(),
    )
    .expect("config");

    let (_sender, mut inbox) = channel();
    match run(&graph, &config, state, None, &context(), &mut inbox).await {
        Ok(Outcome::Finished { state, .. }) => {
            println!("llm_call: {}", state.str(&key("title")).expect("title"));
            for summary in state.list(&key("summaries")).expect("summaries") {
                println!("  - {summary}");
            }
        }
        Ok(_) => println!("llm_call paused or cancelled"),
        Err(failure) => println!("llm_call failed: {}", failure.error),
    }
}
