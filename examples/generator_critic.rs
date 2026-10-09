//! A generator and a critic writing in one shared history, built as a graph
//! and called with `SubGraph`. The critic rejects the first answer, the
//! generator revises it, the critic validates the revision. Whether the
//! critic thinks natively is read from the caller's configuration; here it
//! does not, so its verdict carries its reasoning in `thinking`, which never
//! enters a history.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_graph::{
    Always, Config, CriticSeat, EndLabel, GeneratorCritic, GeneratorSeat, GraphBuilder, Input,
    Kind, Limit, NodeId, Outcome, Output, OutputMode, Schema, SubGraph, Switch, Target, Value,
    channel, run,
};
use br_llm_messages::{Author, Conversation, Entry};
use serde_json::json;

use common::{ScriptedModel, key, quiet_context, structured_step, text_step, user};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let generator = GeneratorSeat {
        author: Author::new("writer").expect("author"),
        model: Arc::new(ScriptedModel::new(vec![
            text_step("Supersteps run nodes in rounds."),
            text_step(
                "Supersteps run the active nodes in parallel rounds, then apply every update at once.",
            ),
        ])),
        output: OutputMode::Text,
        thinking: None,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
    };
    let critic = CriticSeat {
        author: Author::new("reviewer").expect("author"),
        model: Arc::new(ScriptedModel::new(vec![
            structured_step(json!({
                "thinking": "It does not say when updates are applied.",
                "is_valid": false,
                "message": "Say when the updates of a round are applied."
            })),
            structured_step(json!({
                "thinking": "Complete now.",
                "is_valid": true,
                "message": ""
            })),
        ])),
        thinking: Some(Switch::Config(key("critic_thinking"))),
    };
    let duo = GeneratorCritic::new(
        generator,
        critic,
        Limit::Fixed(NonZeroUsize::new(3).expect("non-zero")),
    )
    .graph()
    .expect("generator-critic graph");

    let schema = Schema::builder()
        .state(key("chat"), Kind::Conversation)
        .state(key("answer"), Kind::Conversation)
        .state(key("generations"), Kind::Int)
        .state(key("outcome"), Kind::Str)
        .config(key("native_thinking"), Kind::Bool)
        .build();
    let call = SubGraph::call(duo)
        .input(key("conversation"), Input::From(key("chat")))
        .input(
            key("generator_history"),
            Input::Const(Value::conversation(Conversation::new())),
        )
        .input(
            key("generator_system"),
            Input::Const(Value::str("Answer the question precisely.")),
        )
        .input(
            key("critic_system"),
            Input::Const(Value::str("Reject an answer that leaves out a key fact.")),
        )
        .config(
            key("critic_thinking"),
            Input::Config(key("native_thinking")),
        )
        .output(key("conversation"), Output::Set(key("chat")))
        .output(key("answer"), Output::Set(key("answer")))
        .output(key("generations"), Output::Set(key("generations")))
        .output_end_label(Output::Set(key("outcome")));
    let graph = GraphBuilder::new(schema.clone())
        .entry(NodeId::new("duo").expect("id"))
        .subgraph(NodeId::new("duo").expect("id"), call)
        .edge(
            NodeId::new("duo").expect("id"),
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .input(key("chat"))
        .build()
        .expect("build");

    let mut chat = Conversation::new();
    chat.push_input(user("Explain supersteps."));
    let state = graph
        .start_state([(key("chat"), Value::conversation(chat))])
        .expect("start state");
    let mut values = BTreeMap::new();
    values.insert(key("native_thinking"), Value::bool(false));
    let config = Config::new(&schema, values).expect("config");

    let (_sender, mut inbox) = channel();
    match run(&graph, &config, state, None, &quiet_context(), &mut inbox).await {
        Ok(Outcome::Finished { state, .. }) => {
            println!(
                "generator_critic ended {} after {} generations",
                state.str(&key("outcome")).expect("outcome"),
                state.int(&key("generations")).expect("generations")
            );
            for entry in state.conversation(&key("chat")).expect("chat").entries() {
                match entry {
                    Entry::UserInput(input) => println!("  {input}"),
                    Entry::Turn(turn) => println!("  {turn}"),
                }
            }
            println!(
                "  answer: {}",
                state.conversation(&key("answer")).expect("answer")
            );
        }
        Ok(_) => println!("generator_critic paused or cancelled"),
        Err(failure) => println!("generator_critic failed: {}", failure.error),
    }
}
