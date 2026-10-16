//! Resuming after an interruption.
//!
//! A durable ReAct agent: its model drops the connection in the middle of a
//! reply, so the call fails and its turn is left open in the thread. The
//! host calls again with the same thread and the same opening: the agent
//! reads the thread, fails the open turn (the store repairs it), stores no
//! input twice and answers in the turn of the next rank. A durable agent's
//! resumption is its thread: its checkpoints never hold its conversation.
//!
//! A graph without an agent resumes from its checkpoint: a paused session's
//! checkpoint, written as JSON and read back, and a cancelled superstep run
//! again.

#[allow(dead_code)]
mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_llm_clients_core::OutputMode;
use br_llm_graph::{
    Always, Checkpoint, Config, Context, EndLabel, FnNode, Graph, GraphBuilder, Model, Node,
    NodeFuture, NodeId, Origin, Outcome, Prompt, ReactAgent, Schema, Session, State, StateKey,
    Target, ThreadId, ThreadStore, control, run,
};
use br_llm_messages::{Author, Conversation};

use common::store::MemoryStore;
use common::{opening, quiet_context, scripted_cut, state_key, text_step, user};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    agent_after_a_dropped_connection().await;
    session_paused_then_resumed().await;
    superstep_cancelled_then_run_again().await;
}

async fn agent_after_a_dropped_connection() {
    let store = Arc::new(MemoryStore::new());
    // The first reply streams two events, then the connection drops.
    let client = scripted_cut(
        text_step("Tides follow the moon, twice a day."),
        2,
        vec![text_step("Tides follow the moon, twice a day.")],
    );
    let graph = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model: Model::client(client),
        system: Prompt::text("Answer briefly."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
        thread_store: Some(store.clone()),
    }
    .graph()
    .expect("agent graph");
    let config = Config::new(graph.schema(), Default::default()).expect("config");
    let (opening_key, thread_key, answer): (
        StateKey<Conversation>,
        StateKey<String>,
        StateKey<String>,
    ) = (
        state_key("opening"),
        state_key("thread"),
        state_key("answer"),
    );

    // The host owns the thread: it creates it, then gives its id.
    let thread = ThreadId::new("0199a3c4-5d6e-7f80-9a1b-2c3d4e5f6a7b").expect("a UUIDv7");
    store
        .create(&thread, &Origin::default())
        .await
        .expect("create");
    let start = || {
        graph.start_state([
            opening_key
                .given(opening([user("Why are there tides?")]))
                .expect("opening"),
            thread_key.given(thread.as_str()).expect("thread"),
        ])
    };

    let (_sender, mut channel) = control();
    let first = run(
        &graph,
        &config,
        start().expect("state"),
        None,
        &quiet_context(),
        &mut channel,
    )
    .await;
    let failure = first.err().expect("the dropped connection fails the call");
    println!("rehydration: the first call failed: {}", failure.error);
    store.print(&thread);

    // The same call again: same thread, same opening.
    let (_sender, mut channel) = control();
    match run(
        &graph,
        &config,
        start().expect("state"),
        None,
        &quiet_context(),
        &mut channel,
    )
    .await
    {
        Ok(Outcome::Finished { state, .. }) => {
            println!(
                "rehydration: resumed: {}",
                state.get(&answer).expect("answer")
            );
        }
        Ok(_) => println!("rehydration: the call was paused or cancelled"),
        Err(failure) => println!("rehydration: the call failed again: {}", failure.error),
    }
    store.print(&thread);
}

/// A graph of `a` then `b`, each setting `count`.
fn counting(count: &StateKey<i64>, schema: Schema) -> Arc<Graph> {
    let id = |name: &str| NodeId::new(name).expect("node id");
    Arc::new(
        GraphBuilder::new(schema)
            .entry(id("a"))
            .node(id("a"), set(count, 5))
            .node(id("b"), set(count, 6))
            .edge(id("a"), Always(Target::Node(id("b"))))
            .edge(
                id("b"),
                Always(Target::End(EndLabel::new("done").expect("label"))),
            )
            .build()
            .expect("build"),
    )
}

fn set(count: &StateKey<i64>, value: i64) -> impl Node + use<> {
    let count = count.clone();
    FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let update = count.set(value);
            Box::pin(async move { Ok(vec![update?]) })
        },
    )
}

async fn session_paused_then_resumed() {
    let mut schema = Schema::builder();
    let count: StateKey<i64> = schema.state("count").expect("key");
    let schema = schema.build();
    let graph = counting(&count, schema.clone());
    let config = Config::new(&schema, Default::default()).expect("config");
    let state = graph.start_state([]).expect("state");

    let mut session =
        Session::new(graph.clone(), config.clone(), state, quiet_context()).expect("session");
    session.sender().pause();
    let Outcome::Paused { checkpoint } = session.run_once().await.expect("run") else {
        panic!("expected a pause");
    };
    let json = serde_json::to_string(&checkpoint).expect("json");
    println!(
        "rehydration: paused, the checkpoint written ({} bytes)",
        json.len()
    );
    drop(session);

    let restored: Checkpoint = serde_json::from_str(&json).expect("read back");
    let mut resumed = Session::resume(graph, config, restored, quiet_context()).expect("resume");
    if let Outcome::Finished { state, end } = resumed.run_once().await.expect("run") {
        let count = state.get(&count).expect("count");
        println!("rehydration: resumed to End({end}), count {count}");
    }
}

async fn superstep_cancelled_then_run_again() {
    let mut schema = Schema::builder();
    let count: StateKey<i64> = schema.state("count").expect("key");
    let schema = schema.build();
    // The first run of `work` never ends: the cancel drops it.
    let runs = Arc::new(AtomicUsize::new(0));
    let (write, seen) = (count.clone(), runs.clone());
    let work = FnNode::new(
        move |_s: &State, _c: &Config, _x: &Context| -> NodeFuture<'_> {
            let first = seen.fetch_add(1, Ordering::SeqCst) == 0;
            let update = write.set(9);
            Box::pin(async move {
                if first {
                    futures_util::future::pending::<()>().await;
                }
                Ok(vec![update?])
            })
        },
    );
    let id = NodeId::new("work").expect("node id");
    let graph = GraphBuilder::new(schema.clone())
        .entry(id.clone())
        .node(id.clone(), work)
        .edge(
            id,
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .build()
        .expect("build");
    let config = Config::new(&schema, Default::default()).expect("config");

    let (sender, mut channel) = control();
    sender.cancel();
    let state = graph.start_state([]).expect("state");
    let Ok(Outcome::Cancelled { checkpoint }) =
        run(&graph, &config, state, None, &quiet_context(), &mut channel).await
    else {
        panic!("expected a cancel");
    };
    let before = checkpoint.state.get(&count).expect("count");
    println!("rehydration: cancelled mid-superstep, count still {before}");

    let (_sender, mut channel) = control();
    let resumed = run(
        &graph,
        &config,
        checkpoint.state,
        Some(checkpoint.cursor),
        &quiet_context(),
        &mut channel,
    )
    .await;
    if let Ok(Outcome::Finished { state, end }) = resumed {
        let count = state.get(&count).expect("count");
        println!("rehydration: the superstep ran again, End({end}), count {count}");
    }
}
