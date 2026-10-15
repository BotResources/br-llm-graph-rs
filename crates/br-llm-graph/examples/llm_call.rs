//! One model call per item, then one over the results: `LlmCall` as the body
//! of a `Map` appends a summary per document, in document order, at most two
//! calls at once; a second `LlmCall` reads the summaries as its request and
//! sets a title. Each call keeps no conversation: its answer is data.

#[allow(dead_code)]
mod common;

use std::num::NonZeroUsize;

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, Config, ConfigKey, EndLabel, GraphBuilder, ItemFailure, Limit, LlmCall, LlmOutput,
    LlmRequest, Map, Model, NodeId, Outcome, Prompt, Reads, Schema, StateKey, Target, Value,
    control, run,
};

use common::{context, scripted, text_step};

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
    let mut schema = Schema::builder();
    let documents: StateKey<Vec<String>> = schema.state("documents").expect("key");
    let document: StateKey<String> = schema.state("document").expect("key");
    let summaries: StateKey<Vec<String>> = schema.state("summaries").expect("key");
    let title: StateKey<String> = schema.state("title").expect("key");
    let style: ConfigKey<String> = schema.config("style").expect("key");
    let schema = schema.build();

    // The request is a function of the call's context: it reads the keys
    // declared with it, through their typed keys.
    let mut reads = Reads::new();
    reads.read_state(&document);
    let read = document.clone();
    let summarize = LlmCall {
        model: Model::client(client.clone()),
        system: Prompt::from_config(&style),
        request: LlmRequest::Text(Prompt::from_fn(reads, move |ctx| {
            Ok(format!("Summarize in one sentence:\n{}", ctx.get(&read)?))
        })),
        output_mode: OutputMode::Plain,
        output: LlmOutput::Append(summaries.clone()),
    };
    let each = Map::new(&documents, &document, summarize)
        .max_concurrency(Limit::Fixed(NonZeroUsize::new(2).expect("two")))
        .on_item_failure(ItemFailure::Finish);
    // A list of strings is a text too: its items joined by a blank line.
    let entitle = LlmCall {
        model: Model::client(client),
        system: Prompt::from_config(&style),
        request: LlmRequest::Text(Prompt::from_state_list(&summaries)),
        output_mode: OutputMode::Plain,
        output: LlmOutput::Set(title.clone()),
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
        .input(&documents)
        .output(&summaries)
        .output(&title)
        .build()
        .expect("build");

    let texts = [
        "The tides rise and fall twice a day, pulled by the moon.",
        "At full and new moon, sun and moon pull together: spring tides.",
        "At the quarter moons they pull apart: neap tides.",
    ];
    let state = graph
        .start_state([documents
            .given(texts.into_iter().map(str::to_owned).collect::<Vec<_>>())
            .expect("documents")])
        .expect("start state");
    let config = Config::new(
        &schema,
        [(style.key().clone(), Value::str("Write plain English."))].into(),
    )
    .expect("config");

    let (_sender, mut channel) = control();
    match run(&graph, &config, state, None, &context(), &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => {
            println!("llm_call: {}", state.get(&title).expect("title"));
            for summary in state.get(&summaries).expect("summaries") {
                println!("  - {summary}");
            }
        }
        Ok(_) => println!("llm_call paused or cancelled"),
        Err(failure) => println!("llm_call failed: {}", failure.error),
    }
}
