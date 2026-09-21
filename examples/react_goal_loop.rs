#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_graph::{
    Config, Context, Edge, EndLabel, FnEdge, FnNode, GraphBuilder, Kind, LlmNode, Model,
    NodeFuture, NodeId, Outcome, OutputMode, ReactLoop, Request, Schema, Source, State, Target,
    Value, channel, complete, run, structured, wire,
};
use br_llm_messages::{Author, Conversation};
use serde::Deserialize;
use serde_json::json;

use common::{ScriptedModel, context, key, structured_step, text_step, user};

#[derive(Deserialize)]
struct Verdict {
    achieved: bool,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("num_iter"), Kind::Int)
        .state(key("achieved"), Kind::Bool)
        .config(key("base"), Kind::Str)
        .build();

    let agent_model = Arc::new(ScriptedModel::new(vec![
        text_step("draft one"),
        text_step("draft two"),
    ]));
    let goal_model: Arc<dyn Model> = Arc::new(ScriptedModel::new(vec![
        structured_step(json!({ "achieved": false })),
        structured_step(json!({ "achieved": true })),
    ]));

    let llm = LlmNode {
        key: key("chat"),
        author: Author::new("agent").expect("author"),
        model: agent_model,
        system: vec![Source::Config(key("base"))],
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Text,
    };
    let react = ReactLoop {
        llm: NodeId::new("llm").expect("id"),
        tool_nodes: Vec::new(),
        after: Target::Node(NodeId::new("goal").expect("id")),
    };

    let graph = react
        .add(
            GraphBuilder::new(schema.clone()).entry(NodeId::new("llm").expect("id")),
            llm,
        )
        .expect("partition")
        .node(NodeId::new("goal").expect("id"), goal_node(goal_model))
        .edge(NodeId::new("goal").expect("id"), goal_edge())
        .build()
        .expect("build");

    let (_sender, mut inbox) = channel();
    let outcome = run(
        &graph,
        &config(&schema),
        state(&schema),
        None,
        &context(),
        &mut inbox,
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

fn goal_node(model: Arc<dyn Model>) -> impl br_llm_graph::Node {
    let author = Author::new("agent").expect("author");
    FnNode::new(
        move |state: &State, _config: &Config, ctx: &Context| -> NodeFuture<'_> {
            let model = model.clone();
            let author = author.clone();
            Box::pin(async move {
                let messages = wire(state.conversation(&key("chat"))?, &author)?;
                let request = Request {
                    system: None,
                    messages,
                    tools: Vec::new(),
                    output: OutputMode::Structured { schema: json!({}) },
                };
                let step = complete(model.as_ref(), request, ctx, &key("chat")).await?;
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
            Ok(vec![Target::Node(NodeId::new("llm").expect("id"))])
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
    chat.push_input(user("Give me a good answer."));
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("num_iter"), Value::int(0));
    values.insert(key("achieved"), Value::bool(false));
    State::new(schema.clone(), values).expect("state")
}
