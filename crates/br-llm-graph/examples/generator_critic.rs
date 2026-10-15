//! A generator and a critic writing in one shared history, built as a graph
//! and called with `SubGraph`. The caller passes an empty conversation and a
//! question id; the `prepare` node reads the question from a question bank
//! and opens the conversation. The critic rejects the first answer, the
//! generator revises it, the critic validates the revision. The generator's
//! system text is computed from the state, the critic's review request is
//! read from the configuration. The critic's client does not reason natively
//! (`ReasoningLevel::Off`), so its verdict carries its reasoning in
//! `thinking`, which never enters a history. The caller reads plain data: the
//! answer's text, whether it was validated and how many answers were written.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{OutputMode, ReasoningLevel};
use br_llm_graph::{
    Always, Config, Context, Count, CriticSeat, EndLabel, GeneratorCritic, GeneratorSeat,
    GraphBuilder, Input, Kind, Node, NodeFuture, NodeId, Outcome, Output, Prompt, Reads, Schema,
    State, StateKey, SubGraph, Target, Update, Value, control, run,
};
use br_llm_messages::{Author, Conversation};
use serde_json::json;

use common::{
    config_text, key, quiet_context, scripted, scripted_at, structured_step, text_step, user,
};

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
    // The generator's system text reads `question_id`, declared with it: an
    // input of the graph.
    let mut reads = Reads::new();
    let question_id: StateKey<String> = reads.state("question_id").expect("key");
    let generator = GeneratorSeat {
        author: Author::new("writer", "writer").expect("author"),
        system: Prompt::from_fn(reads, move |ctx| {
            Ok(format!(
                "Answer question {} precisely.",
                ctx.get(&question_id)?
            ))
        }),
        client: scripted(vec![
            text_step("Supersteps run nodes in rounds."),
            text_step(
                "Supersteps run the active nodes in parallel rounds, then apply every update at once.",
            ),
        ]),
        output_mode: OutputMode::Plain,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
    };
    let critic = CriticSeat {
        review_request: Some(config_text("review_request")),
        ..CriticSeat::new(
            Author::new("reviewer", "reviewer").expect("author"),
            Arc::new(scripted_at(
                ReasoningLevel::Off,
                vec![
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
                ],
            )),
            Prompt::text("Reject an answer that leaves out a key fact."),
        )
    };
    let bank = Arc::new(QuestionBank(BTreeMap::from([(
        "q1",
        "Explain supersteps.",
    )])));
    let duo = GeneratorCritic {
        prepare: Some(Box::new(OpenQuestion { bank })),
        ..GeneratorCritic::new(generator, critic, Count::Fixed(3))
    }
    .graph()
    .expect("generator-critic graph");

    let schema = Schema::builder()
        .state_by_name(key("question"), Kind::Str)
        .state_by_name(key("answer"), Kind::Str)
        .state_by_name(key("validated"), Kind::Bool)
        .state_by_name(key("generations"), Kind::Int)
        .state_by_name(key("outcome"), Kind::Str)
        .config_by_name(key("review_request"), Kind::Str)
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
    values.insert(
        key("review_request"),
        Value::str("Does the answer above leave out a key fact?"),
    );
    let config = Config::new(&schema, values).expect("config");

    let (_sender, mut channel) = control();
    match run(&graph, &config, state, None, &quiet_context(), &mut channel).await {
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
