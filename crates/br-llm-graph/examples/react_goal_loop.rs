//! A goal loop around a durable ReAct agent called with `SubGraph`: after
//! each answer, a goal node of the host's asks a judge model whether the
//! answer reaches the goal, and either ends the run or calls the agent again
//! in the same thread, its next opening a runtime input saying the goal is
//! not reached, up to three tries.
//!
//! The goal node is written on the graph's helpers: `complete` calls the
//! judge with the observer as sink, `structured` reads its verdict.

#[allow(dead_code)]
mod common;

use std::sync::Arc;

use br_llm_clients_core::{
    Client, ClientRequest, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_graph::{
    Always, Config, Context, Edge, EndLabel, FnEdge, FnNode, Graph, GraphBuilder, Model, Node,
    NodeFuture, NodeId, Outcome, Prompt, ReactAgent, Schema, State, StateKey, SubGraph, Target,
    ThreadId, complete, control, run, structured,
};
use br_llm_messages::{Author, Conversation, Text, WireMessage, WireUserBlock};
use serde::Deserialize;
use serde_json::json;

use common::store::MemoryStore;
use common::{
    opening, quiet_context, runtime_input, scripted, state_key, structured_step, text_step, user,
};

#[derive(Deserialize)]
struct Verdict {
    achieved: bool,
}

/// The keys of the loop's graph.
#[derive(Clone)]
struct Keys {
    next: StateKey<Conversation>,
    thread: StateKey<String>,
    answer: StateKey<String>,
    tries: StateKey<i64>,
    achieved: StateKey<bool>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let store = Arc::new(MemoryStore::new());
    let agent = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        model: Model::client(scripted(vec![
            text_step("A draft."),
            text_step("A complete answer, with its reasons."),
        ])),
        system: Prompt::text("Draft an answer, then improve it when asked."),
        history: None,
        output_mode: OutputMode::Plain,
        tools: Vec::new(),
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
    let judge = scripted(vec![
        structured_step(json!({ "achieved": false })),
        structured_step(json!({ "achieved": true })),
    ]);

    let mut schema = Schema::builder();
    let keys = Keys {
        next: schema.state("next").expect("key"),
        thread: schema.state("thread").expect("key"),
        answer: schema.state("answer").expect("key"),
        tries: schema.state("tries").expect("key"),
        achieved: schema.state("achieved").expect("key"),
    };
    let schema = schema.build();
    let graph = goal_loop(schema.clone(), &keys, agent, judge);
    let state = graph
        .start_state([keys
            .next
            .given(opening([user("Give me a good answer.")]))
            .expect("opening")])
        .expect("start state");
    let config = Config::new(&schema, Default::default()).expect("config");

    let (_sender, mut channel) = control();
    match run(&graph, &config, state, None, &quiet_context(), &mut channel).await {
        Ok(Outcome::Finished { state, end }) => {
            println!(
                "react_goal_loop finished on End({end}) after {} tries: {}",
                state.get(&keys.tries).expect("tries"),
                state.get(&keys.answer).expect("answer")
            );
            let thread = state.get(&keys.thread).expect("thread");
            store.print(&ThreadId::new(thread).expect("a thread id"));
        }
        Ok(_) => println!("react_goal_loop paused or cancelled"),
        Err(failure) => println!("react_goal_loop failed: {}", failure.error),
    }
}

/// `agent` answers `next` in `thread`; `goal` judges the answer.
fn goal_loop(schema: Schema, keys: &Keys, agent: Arc<Graph>, judge: Arc<dyn Client>) -> Graph {
    let call = SubGraph::call(agent)
        .input_from(&state_key("opening"), &keys.next)
        .input_from(&state_key("thread"), &keys.thread)
        .output_set(&state_key("answer"), &keys.answer)
        .output_set(&state_key("thread"), &keys.thread);
    let id = |name: &str| NodeId::new(name).expect("node id");
    GraphBuilder::new(schema)
        .entry(id("agent"))
        .subgraph(id("agent"), call)
        .edge(id("agent"), Always(Target::Node(id("goal"))))
        .node(id("goal"), goal_node(keys.clone(), judge))
        .edge(id("goal"), goal_edge(keys.clone()))
        .input(&keys.next)
        .build()
        .expect("build")
}

/// The shape of the judge's verdict: whether the goal is achieved.
fn verdict_shape() -> StructuredShape {
    let achieved = Parameter::new(
        "achieved",
        "whether the answer reaches the goal",
        true,
        ParameterKind::Boolean,
    )
    .expect("parameter");
    StructuredShape::new(
        "goal_verdict",
        None,
        Parameters::new(vec![achieved]).expect("parameters"),
    )
    .expect("shape")
}

/// Asks the judge whether the answer reaches the goal; when it does not,
/// writes the next opening, a runtime input asking for a better answer.
fn goal_node(keys: Keys, judge: Arc<dyn Client>) -> impl Node {
    FnNode::new(
        move |state: &State, _: &Config, ctx: &Context| -> NodeFuture<'_> {
            let (keys, judge) = (keys.clone(), judge.clone());
            Box::pin(async move {
                let answer = Text::new(state.get(&keys.answer)?)?;
                let request = ClientRequest::new(
                    Some(Text::new("Does this answer reach the goal?")?),
                    vec![WireMessage::User {
                        content: vec![WireUserBlock::Text { text: answer }],
                    }],
                    Vec::new(),
                    OutputMode::Structured(verdict_shape()),
                )?;
                let step = complete(judge.as_ref(), &request, ctx, keys.answer.key()).await?;
                let verdict: Verdict = structured(&step)?;
                let tries = state.get(&keys.tries)? + 1;
                let mut updates =
                    vec![keys.tries.set(tries)?, keys.achieved.set(verdict.achieved)?];
                if !verdict.achieved {
                    let retry = format!("Not yet (try {tries}): improve the answer.");
                    updates.push(keys.next.set(opening([runtime_input("goal", &retry)]))?);
                }
                Ok(updates)
            })
        },
    )
}

fn goal_edge(keys: Keys) -> impl Edge {
    FnEdge::new(move |state: &State, _: &Config| {
        let label = match (state.get(&keys.achieved)?, state.get(&keys.tries)?) {
            (true, _) => "achieved",
            (false, tries) if tries >= 3 => "gave_up",
            (false, _) => return Ok(vec![Target::Node(NodeId::new("agent")?)]),
        };
        Ok(vec![Target::End(EndLabel::new(label)?)])
    })
}
