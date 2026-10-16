//! A durable ReAct agent called from another graph with `SubGraph`, twice,
//! in one thread. The first call gives the empty `thread`: the store creates
//! a thread under an id from the id source, and the call gives the id back.
//! The second call gives that id and a new question: the agent reads the
//! thread, so the model sees the first exchange, and writes its turn there.
//!
//! The caller maps its keys to the agent's through typed keys; the model is
//! a `Client` of br-llm-clients-core, whose settings, its level of reasoning
//! included, are fixed when the host builds it.

#[allow(dead_code)]
mod common;

use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, Config, ConfigKey, EndLabel, Graph, GraphBuilder, Model, NodeId, Outcome, Prompt,
    ReactAgent, Schema, StateKey, SubGraph, Target, ThreadId, Value, control, run,
};
use br_llm_messages::{Author, Conversation};
use serde_json::json;

use common::store::MemoryStore;
use common::tools::SearchTool;
use common::{
    config_key, opening, quiet_context, scripted, state_key, text_step, tool_call_step, user,
};

/// The caller's keys.
struct Keys {
    question: StateKey<Conversation>,
    thread: StateKey<String>,
    answer: StateKey<String>,
    prompt: ConfigKey<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = scripted(vec![
        tool_call_step("c1", "search", json!({ "q": "rust" })),
        text_step("Rust is a systems language; here is what I found."),
        text_step("Its borrow checker, as the search said."),
    ]);
    let store = Arc::new(MemoryStore::new());

    // The agent's graph: inputs `opening` and `thread`, outputs `answer`
    // and `thread`, configuration `base`, the system text it reads.
    let base: ConfigKey<String> = config_key("base");
    let agent = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model: Model::client(client.clone()),
        system: Prompt::from_config(&base),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(SearchTool::new())],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
        thread_store: Some(store.clone()),
    }
    .graph()
    .expect("agent graph");

    let mut schema = Schema::builder();
    let keys = Keys {
        question: schema.state("question").expect("key"),
        thread: schema.state("thread").expect("key"),
        answer: schema.state("answer").expect("key"),
        prompt: schema.config("prompt").expect("key"),
    };
    let schema = schema.build();
    let graph = caller(schema.clone(), &keys, agent);
    let config = Config::new(
        &schema,
        [(
            keys.prompt.key().clone(),
            Value::str("You are a research assistant."),
        )]
        .into(),
    )
    .expect("config");

    // The first call: a new thread.
    let thread = ask(&graph, &config, &keys, "Find me something about Rust.", "").await;
    // The second: the same thread, a new question.
    let thread = ask(&graph, &config, &keys, "What stands out?", &thread).await;

    let requests = client.log();
    let last = requests.last().expect("a call");
    println!(
        "react_agent: the last request sent {} messages, the first exchange included",
        last.request().messages().len()
    );
    store.print(&ThreadId::new(thread).expect("a thread id"));
}

/// The caller: one node, `agent`, that calls the agent's graph.
fn caller(schema: Schema, keys: &Keys, agent: Arc<Graph>) -> Graph {
    let call = SubGraph::call(agent)
        .input_from(&state_key("opening"), &keys.question)
        .input_from(&state_key("thread"), &keys.thread)
        .config_from(&config_key("base"), &keys.prompt)
        .output_set(&state_key("answer"), &keys.answer)
        .output_set(&state_key("thread"), &keys.thread);
    let id = NodeId::new("agent").expect("node id");
    GraphBuilder::new(schema)
        .entry(id.clone())
        .subgraph(id.clone(), call)
        .edge(
            id,
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .input(&keys.question)
        .input(&keys.thread)
        .output(&keys.answer)
        .output(&keys.thread)
        .build()
        .expect("build")
}

/// One call of the agent on `question` in `thread`: the id of its thread.
async fn ask(graph: &Graph, config: &Config, keys: &Keys, question: &str, thread: &str) -> String {
    let state = graph
        .start_state([
            keys.question
                .given(opening([user(question)]))
                .expect("question"),
            keys.thread.given(thread).expect("thread"),
        ])
        .expect("start state");
    let (_sender, mut channel) = control();
    match run(graph, config, state, None, &quiet_context(), &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => {
            println!("react_agent: {}", state.get(&keys.answer).expect("answer"));
            state.get(&keys.thread).expect("thread").to_owned()
        }
        Ok(_) => panic!("the call was paused or cancelled"),
        Err(failure) => panic!("the call failed: {}", failure.error),
    }
}
