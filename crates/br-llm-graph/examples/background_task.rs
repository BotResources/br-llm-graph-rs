//! A durable ReAct agent that starts background work and answers at once,
//! called by a session's graph. The graph keeps the inputs that came since
//! the agent's last call in `inbox`, its `opening`; after each call `settle`
//! empties it and ends the run `waiting`, or `done` once the agent has read
//! the completion. The session serves: the completion arrives between runs
//! through its control sender, as a runtime input; the session applies it
//! and runs again, and the agent answers it in the same thread. A cancel
//! ends serving with the session's checkpoint.

#[allow(dead_code)]
mod common;

use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, Config, Context, Edge, EndLabel, Ended, FnEdge, FnNode, Graph, GraphBuilder, Model,
    NodeFuture, NodeId, Observer, Origin, Prompt, ReactAgent, Schema, Session, Start, State,
    StateKey, SubGraph, Target, ThreadId,
};
use br_llm_messages::{Author, Conversation, Entry, UserSource};
use futures_channel::mpsc;
use futures_util::StreamExt;
use serde_json::json;

use common::store::MemoryStore;
use common::tools::StartTaskTool;
use common::{
    SeqIds, opening, runtime_input, scripted, state_key, text_step, tool_call_step, user,
};

/// The keys of the session's graph.
struct Keys {
    inbox: StateKey<Conversation>,
    thread: StateKey<String>,
    answer: StateKey<String>,
    status: StateKey<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let client = scripted(vec![
        tool_call_step("c1", "start_task", json!({})),
        text_step("Task started; I will report when it is done."),
        text_step("The task is complete."),
    ]);
    let store = Arc::new(MemoryStore::new());
    let agent = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model: Model::client(client),
        system: Prompt::text("Kick off background work, then report on it."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(StartTaskTool::new())],
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
        inbox: schema.state("inbox").expect("key"),
        thread: schema.state("thread").expect("key"),
        answer: schema.state("answer").expect("key"),
        status: schema.state("status").expect("key"),
    };
    let schema = schema.build();
    let graph = Arc::new(session_graph(schema.clone(), &keys, agent));
    let state = graph
        .start_state([keys
            .inbox
            .given(opening([user("Please run the long job.")]))
            .expect("inbox")])
        .expect("state");
    let config = Config::new(&schema, Default::default()).expect("config");

    // The labels the session's runs end with, as an observer hears them.
    let (ends, mut ended) = mpsc::unbounded();
    let context = Context::new(Arc::new(RunEnds(ends)), Arc::new(SeqIds::new()));
    let session = Session::new(graph, config, state, context).expect("session");
    let sender = session.sender();
    let serving = tokio::spawn(session.serve(Start::Now));
    println!(
        "background_task: the first run ended {:?}",
        ended.next().await
    );

    // The work ends: its completion reaches the session as a runtime input.
    sender.send(
        keys.inbox.key().clone(),
        runtime_input("task_done", "task-1 finished"),
    );
    println!(
        "background_task: the next run ended {:?}",
        ended.next().await
    );

    sender.cancel();
    let Ended::Cancelled { checkpoint } = serving.await.expect("serving") else {
        panic!("serving failed");
    };
    let state = checkpoint.state;
    println!(
        "background_task: the agent said {:?}",
        state.get(&keys.answer).expect("answer")
    );
    store.print(&ThreadId::new(state.get(&keys.thread).expect("thread")).expect("a thread id"));
}

/// Sends the end label of each top-level run of the session.
struct RunEnds(mpsc::UnboundedSender<String>);

impl Observer for RunEnds {
    fn run_finished(&self, origin: &Origin, end: &EndLabel) {
        if origin.occurrence.is_root() {
            drop(self.0.unbounded_send(end.as_str().to_owned()));
        }
    }
}

/// `agent` answers what `inbox` holds, in `thread`; `settle` empties
/// `inbox` and sets `status`, which the run ends with.
fn session_graph(schema: Schema, keys: &Keys, agent: Arc<Graph>) -> Graph {
    let call = SubGraph::call(agent)
        .input_from(&state_key("opening"), &keys.inbox)
        .input_from(&state_key("thread"), &keys.thread)
        .output_set(&state_key("answer"), &keys.answer)
        .output_set(&state_key("thread"), &keys.thread);
    let (inbox, status) = (keys.inbox.clone(), keys.status.clone());
    let settle = FnNode::new(
        move |state: &State, _: &Config, _: &Context| -> NodeFuture<'_> {
            let completed = state.get(&inbox).map(|inbox| {
                inbox.entries().iter().any(|entry| {
                    matches!(entry, Entry::UserInput(input)
                    if matches!(input.source(), UserSource::Runtime { .. }))
                })
            });
            let updates = completed.and_then(|completed| {
                Ok(vec![
                    inbox.set(Conversation::new())?,
                    status.set(if completed { "done" } else { "waiting" })?,
                ])
            });
            Box::pin(async move { Ok(updates?) })
        },
    );
    let id = |name: &str| NodeId::new(name).expect("node id");
    GraphBuilder::new(schema)
        .entry(id("agent"))
        .subgraph(id("agent"), call)
        .edge(id("agent"), Always(Target::Node(id("settle"))))
        .node(id("settle"), settle)
        .edge(id("settle"), end_on(keys.status.clone()))
        .input(&keys.inbox)
        .build()
        .expect("build")
}

/// Ends the run with the label `status` holds.
fn end_on(status: StateKey<String>) -> impl Edge {
    FnEdge::new(move |state: &State, _config: &Config| {
        Ok(vec![Target::End(EndLabel::new(state.get(&status)?)?)])
    })
}
