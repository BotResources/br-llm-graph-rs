//! A goal loop around a ReAct agent called with `SubGraph`: after each
//! answer, a goal node asks a judge model whether the goal is achieved and
//! either ends the run or calls the agent again on the same history, up to
//! three tries.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{
    Client, ClientRequest, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_graph::{
    Always, Config, Context, Edge, EndLabel, FnEdge, FnNode, GraphBuilder, Input, Kind, NodeFuture,
    NodeId, Outcome, Output, ReactAgent, Schema, Source, State, SubGraph, Target, Value, complete,
    control, run, structured, wire,
};
use br_llm_messages::{Author, Conversation};
use serde::Deserialize;
use serde_json::json;

use common::{context, key, scripted, structured_step, text_step, user};

#[derive(Deserialize)]
struct Verdict {
    achieved: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let schema = Schema::builder()
        .state_by_name(key("chat"), Kind::Conversation)
        .state_by_name(key("num_iter"), Kind::Int)
        .state_by_name(key("achieved"), Kind::Bool)
        .config_by_name(key("base"), Kind::Str)
        .build();

    let agent_client = scripted(vec![text_step("draft one"), text_step("draft two")]);
    let goal_client: Arc<dyn Client> = scripted(vec![
        structured_step(json!({ "achieved": false })),
        structured_step(json!({ "achieved": true })),
    ]);

    // Input `history`; configuration `base`. No tools: each call is one
    // model reply.
    let agent = ReactAgent {
        author: Author::new("agent", "agent").expect("author"),
        client: agent_client,
        system: vec![Source::Config(key("base"))],
        output_mode: OutputMode::Plain,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        inputs: Vec::new(),
    }
    .graph()
    .expect("agent graph");
    let call = SubGraph::call(agent)
        .input(key("history"), Input::From(key("chat")))
        .config(key("base"), Input::Config(key("base")))
        .output(key("history"), Output::Set(key("chat")));

    let graph = GraphBuilder::new(schema.clone())
        .entry(NodeId::new("agent").expect("id"))
        .subgraph(NodeId::new("agent").expect("id"), call)
        .edge(
            NodeId::new("agent").expect("id"),
            Always(Target::Node(NodeId::new("goal").expect("id"))),
        )
        .node(NodeId::new("goal").expect("id"), goal_node(goal_client))
        .edge(NodeId::new("goal").expect("id"), goal_edge())
        .build()
        .expect("build");

    let (_sender, mut channel) = control();
    let outcome = run(
        &graph,
        &config(&schema),
        state(&schema),
        None,
        &context(),
        &mut channel,
    )
    .await;
    match outcome {
        Ok(Outcome::Finished { state, end }) => println!(
            "react_goal_loop finished on End({end}) after num_iter={}",
            state.int(&key("num_iter")).expect("int")
        ),
        Ok(_) => println!("react_goal_loop paused or cancelled"),
        Err(failure) => println!("react_goal_loop failed: {}", failure.error),
    }
}

/// The shape of the judge's verdict: whether the goal is achieved.
fn verdict_shape() -> StructuredShape {
    StructuredShape::new(
        "goal_verdict",
        None,
        Parameters::new(vec![
            Parameter::new(
                "achieved",
                "whether the answer reaches the goal",
                true,
                ParameterKind::Boolean,
            )
            .expect("parameter"),
        ])
        .expect("parameters"),
    )
    .expect("shape")
}

/// Asks the judge whether the goal is achieved, with the graph's helpers:
/// the history rendered for the agent, the call streaming to the observer,
/// the structured block read into `Verdict`.
fn goal_node(client: Arc<dyn Client>) -> impl br_llm_graph::Node {
    let author = Author::new("agent", "agent").expect("author");
    let shape = verdict_shape();
    FnNode::new(
        move |state: &State, _config: &Config, ctx: &Context| -> NodeFuture<'_> {
            let client = client.clone();
            let author = author.clone();
            let shape = shape.clone();
            Box::pin(async move {
                let messages = wire(state.conversation(&key("chat"))?, &author)?;
                let request =
                    ClientRequest::new(None, messages, Vec::new(), OutputMode::Structured(shape))?;
                let step = complete(client.as_ref(), &request, ctx, &key("chat")).await?;
                let verdict: Verdict = structured(&step)?;
                let next = state.int(&key("num_iter"))? + 1;
                Ok(vec![
                    set_bool(key("achieved"), verdict.achieved),
                    set_int(key("num_iter"), next),
                ])
            })
        },
    )
}

fn set_bool(key: br_llm_graph::Key, value: bool) -> br_llm_graph::Update {
    br_llm_graph::Update::Set {
        key,
        value: Value::bool(value),
    }
}

fn set_int(key: br_llm_graph::Key, value: i64) -> br_llm_graph::Update {
    br_llm_graph::Update::Set {
        key,
        value: Value::int(value),
    }
}

fn goal_edge() -> impl Edge {
    FnEdge::new(|state: &State, _config: &Config| {
        let achieved = state.bool(&key("achieved"))?;
        let num_iter = state.int(&key("num_iter"))?;
        if achieved {
            Ok(vec![Target::End(EndLabel::new("achieved").expect("label"))])
        } else if num_iter >= 3 {
            Ok(vec![Target::End(EndLabel::new("gave_up").expect("label"))])
        } else {
            Ok(vec![Target::Node(NodeId::new("agent").expect("id"))])
        }
    })
}

fn config(schema: &Schema) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("Draft an answer."));
    Config::new(schema, values).expect("config")
}

fn state(schema: &Schema) -> State {
    let mut chat = Conversation::new();
    chat.push_input(user("Give me a good answer."))
        .expect("an input id unique in the conversation");
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("num_iter"), Value::int(0));
    values.insert(key("achieved"), Value::bool(false));
    State::new(schema.clone(), values).expect("state")
}
