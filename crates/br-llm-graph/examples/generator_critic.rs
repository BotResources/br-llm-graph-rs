//! A generator and a critic writing in one shared history, built as a graph
//! and called with `SubGraph`. The caller passes an empty conversation and a
//! question id; the `prepare` node reads the question from a question bank
//! and opens the conversation. The critic rejects the first answer, the
//! generator revises it, the critic validates the revision. The generator's
//! system text is computed from the state, the critic's review request is
//! read from the configuration, and whether the critic thinks natively too.
//! Its verdict carries its reasoning in `thinking`, which never enters a
//! history. The caller reads plain data: the answer's text, whether it was
//! validated and how many answers were written.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_graph::{
    Always, Config, Context, Count, CriticSeat, EndLabel, GeneratorCritic, GeneratorSeat,
    GraphBuilder, Input, Kind, Node, NodeFuture, NodeId, Outcome, Output, OutputMode, Schema,
    Source, State, SubGraph, Switch, Target, Update, Value, channel, run,
};
use br_llm_messages::{Author, Conversation};
use serde_json::json;

use common::{ScriptedModel, key, quiet_context, structured_step, text_step, user};

/// A question bank standing for a service the `prepare` node calls.
struct QuestionBank(BTreeMap<&'static str, &'static str>);

/// Opens the conversation with the question `question_id` names.
struct OpenQuestion {
    bank: Arc<QuestionBank>,
}

impl Node for OpenQuestion {
    fn run<'a>(
        &'a self,
        state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async move {
            let id = state.str(&key("question_id"))?;
            let question = self
                .bank
                .0
                .get(id)
                .ok_or_else(|| format!("no question {id}"))?;
            Ok(vec![Update::Input {
                key: key("conversation"),
                input: user(question),
            }])
        })
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let generator = GeneratorSeat {
        author: Author::new("writer", "writer").expect("author"),
        system: Source::Fn(Arc::new(|state, _config| {
            Ok(format!(
                "Answer question {} precisely.",
                state.str(&key("question_id"))?
            ))
        })),
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
        enabled: None,
    };
    let critic = CriticSeat {
        thinking: Some(Switch::Config(key("critic_thinking"))),
        review_request: Some(Source::Config(key("review_request"))),
        ..CriticSeat::new(
            Author::new("reviewer", "reviewer").expect("author"),
            Arc::new(ScriptedModel::new(vec![
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
            Source::Const("Reject an answer that leaves out a key fact.".to_owned()),
        )
    };
    let bank = Arc::new(QuestionBank(BTreeMap::from([(
        "q1",
        "Explain supersteps.",
    )])));
    let duo = GeneratorCritic {
        inputs: vec![(key("question_id"), Kind::Str)],
        prepare: Some(Box::new(OpenQuestion { bank })),
        ..GeneratorCritic::new(generator, critic, Count::Fixed(3))
    }
    .graph()
    .expect("generator-critic graph");

    let schema = Schema::builder()
        .state(key("question"), Kind::Str)
        .state(key("answer"), Kind::Str)
        .state(key("validated"), Kind::Bool)
        .state(key("generations"), Kind::Int)
        .state(key("outcome"), Kind::Str)
        .config(key("native_thinking"), Kind::Bool)
        .config(key("review_request"), Kind::Str)
        .build();
    let call = SubGraph::call(duo)
        .input(
            key("conversation"),
            Input::Const(Value::conversation(Conversation::new())),
        )
        .input(
            key("generator_history"),
            Input::Const(Value::conversation(Conversation::new())),
        )
        .input(key("question_id"), Input::From(key("question")))
        .config(
            key("critic_thinking"),
            Input::Config(key("native_thinking")),
        )
        .config(key("review_request"), Input::Config(key("review_request")))
        .output(key("answer"), Output::Set(key("answer")))
        .output(key("is_validated"), Output::Set(key("validated")))
        .output(key("nb_generations"), Output::Set(key("generations")))
        .output_end_label(Output::Set(key("outcome")));
    let graph = GraphBuilder::new(schema.clone())
        .entry(NodeId::new("duo").expect("id"))
        .subgraph(NodeId::new("duo").expect("id"), call)
        .edge(
            NodeId::new("duo").expect("id"),
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .input(key("question"))
        .build()
        .expect("build");

    let state = graph
        .start_state([(key("question"), Value::str("q1"))])
        .expect("start state");
    let mut values = BTreeMap::new();
    values.insert(key("native_thinking"), Value::bool(false));
    values.insert(
        key("review_request"),
        Value::str("Does the answer above leave out a key fact?"),
    );
    let config = Config::new(&schema, values).expect("config");

    let (_sender, mut inbox) = channel();
    match run(&graph, &config, state, None, &quiet_context(), &mut inbox).await {
        Ok(Outcome::Finished { state, .. }) => {
            println!(
                "generator_critic ended {} after {} generations (validated: {})",
                state.str(&key("outcome")).expect("outcome"),
                state.int(&key("generations")).expect("generations"),
                state.bool(&key("validated")).expect("validated"),
            );
            println!("  answer: {}", state.str(&key("answer")).expect("answer"));
        }
        Ok(_) => println!("generator_critic paused or cancelled"),
        Err(failure) => println!("generator_critic failed: {}", failure.error),
    }
}
