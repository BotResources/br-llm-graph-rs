#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_graph::{
    Always, Config, Context, Edge, EndLabel, FnEdge, FnNode, GraphBuilder, Kind, LlmNode, Model,
    NodeFuture, NodeId, Outcome, OutputMode, Request, Schema, Source, State, Target, Update, Value,
    channel, complete, run, structured, wire,
};
use br_llm_messages::{Author, Conversation, Text, UserBlock, UserInput, UserSource};
use serde::Deserialize;
use serde_json::json;

use common::{ScriptedModel, context, key, structured_step, text_step, user};

#[derive(Deserialize)]
struct Verdict {
    validated: bool,
    note: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("is_validated"), Kind::Bool)
        .state(key("num_iter"), Kind::Int)
        .config(key("base"), Kind::Str)
        .build();

    let generator = LlmNode {
        key: key("chat"),
        author: Author::new("generator").expect("author"),
        model: Arc::new(ScriptedModel::new(vec![
            text_step("First attempt at the answer."),
            text_step("Revised, more detailed answer."),
        ])),
        system: vec![Source::Config(key("base"))],
        tools: Vec::new(),
        enabled: None,
        output: OutputMode::Text,
    };
    let critic_model: Arc<dyn Model> = Arc::new(ScriptedModel::new(vec![
        structured_step(json!({ "validated": false, "note": "add a concrete example" })),
        structured_step(json!({ "validated": true, "note": "looks good" })),
    ]));

    let graph = GraphBuilder::new(schema.clone())
        .entry(NodeId::new("generator").expect("id"))
        .node(NodeId::new("generator").expect("id"), generator)
        .node(
            NodeId::new("critic").expect("id"),
            critic_node(critic_model),
        )
        .edge(
            NodeId::new("generator").expect("id"),
            Always(Target::Node(NodeId::new("critic").expect("id"))),
        )
        .edge(NodeId::new("critic").expect("id"), critic_edge())
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
            "generator_critic finished on End({end}) after num_iter={}",
            state.int(&key("num_iter")).expect("int")
        ),
        Ok(_) => println!("generator_critic paused or cancelled"),
        Err(failure) => println!("generator_critic failed: {}", failure.error),
    }
}

fn critic_node(model: Arc<dyn Model>) -> impl br_llm_graph::Node {
    let author = Author::new("generator").expect("author");
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
                let mut updates = vec![Update::Set {
                    key: key("is_validated"),
                    value: Value::bool(verdict.validated),
                }];
                if !verdict.validated {
                    let next = state.int(&key("num_iter"))? + 1;
                    updates.push(Update::Set {
                        key: key("num_iter"),
                        value: Value::int(next),
                    });
                    let instruction = UserInput::new(
                        UserSource::runtime("critic")?,
                        None,
                        vec![UserBlock::text(Text::new(format!(
                            "revise: {}",
                            verdict.note
                        ))?)],
                    )?;
                    updates.push(Update::Input {
                        key: key("chat"),
                        input: instruction,
                    });
                }
                Ok(updates)
            })
        },
    )
}

fn critic_edge() -> impl Edge {
    FnEdge::new(|state: &State, _config: &Config| {
        if state.bool(&key("is_validated"))? {
            Ok(vec![Target::End(
                EndLabel::new("validated").expect("label"),
            )])
        } else if state.int(&key("num_iter"))? >= 3 {
            Ok(vec![Target::End(EndLabel::new("gave_up").expect("label"))])
        } else {
            Ok(vec![Target::Node(NodeId::new("generator").expect("id"))])
        }
    })
}

fn config(schema: &Schema) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("Answer the user's question well."));
    Config::new(schema, values).expect("config")
}

fn state(schema: &Schema) -> State {
    let mut chat = Conversation::new();
    chat.push_input(user("Explain supersteps."));
    let mut values = BTreeMap::new();
    values.insert(key("chat"), Value::conversation(chat));
    values.insert(key("is_validated"), Value::bool(false));
    values.insert(key("num_iter"), Value::int(0));
    State::new(schema.clone(), values).expect("state")
}
