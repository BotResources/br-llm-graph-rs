//! A durable generator-critic called with `SubGraph`: a generator and a
//! critic writing in one conversation, its thread.
//!
//! The caller passes an empty opening and a question id; the `prepare` node
//! reads the question from a question bank and writes it into the opening,
//! the trigger of the call. The generator's system text reads the question
//! id; the rule check asks for a revision of an answer shorter than five
//! words, before the critic; the critic rejects the next answer, the
//! generator revises it, the critic accepts the revision. The critic's
//! client does not reason natively (`ReasoningLevel::Off`), so the verdict
//! shape asks for `thinking`, which is never recorded. The caller reads
//! plain data: the answer, whether it was validated, how many generations it
//! took, and the thread.

#[allow(dead_code)]
mod common;

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::{OutputMode, ReasoningLevel};
use br_llm_graph::{
    Always, Config, ConfigKey, Context, Count, CriticSeat, EndLabel, GeneratorCritic,
    GeneratorSeat, Graph, GraphBuilder, Model, Node, NodeFuture, NodeId, Outcome, Prompt, Reads,
    RuleCheck, Ruling, Schema, State, StateKey, SubGraph, Target, ThreadId, Value, control, run,
};
use br_llm_messages::{Author, Conversation, Text};
use serde_json::json;

use common::store::MemoryStore;
use common::{
    config_key, quiet_context, scripted, scripted_at, state_key, structured_step, text_step, user,
};

/// A question bank standing for a service the `prepare` node calls.
struct QuestionBank(BTreeMap<&'static str, &'static str>);

/// Writes the question `question_id` names into `opening`.
struct OpenQuestion {
    bank: Arc<QuestionBank>,
    question_id: StateKey<String>,
    opening: StateKey<Conversation>,
}

impl Node for OpenQuestion {
    fn run<'a>(&'a self, state: &'a State, _: &'a Config, _: &'a Context) -> NodeFuture<'a> {
        Box::pin(async move {
            let id = state.get(&self.question_id)?;
            let question = self.bank.0.get(id).ok_or(format!("no question {id}"))?;
            Ok(vec![self.opening.input(user(question))])
        })
    }
}

/// The caller's keys.
struct Keys {
    question: StateKey<String>,
    thread: StateKey<String>,
    answer: StateKey<String>,
    validated: StateKey<bool>,
    generations: StateKey<i64>,
    outcome: StateKey<String>,
    review_request: ConfigKey<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let store = Arc::new(MemoryStore::new());
    let duo = generator_critic(store.clone());

    let mut schema = Schema::builder();
    let keys = Keys {
        question: schema.state("question").expect("key"),
        thread: schema.state("thread").expect("key"),
        answer: schema.state("answer").expect("key"),
        validated: schema.state("validated").expect("key"),
        generations: schema.state("generations").expect("key"),
        outcome: schema.state("outcome").expect("key"),
        review_request: schema.config("review_request").expect("key"),
    };
    let schema = schema.build();
    let call = SubGraph::call(duo)
        .input_value(&state_key::<Conversation>("opening"), Conversation::new())
        .expect("an empty opening")
        .input_from(&state_key("question_id"), &keys.question)
        .input_from(&state_key("thread"), &keys.thread)
        .config_from(&config_key("review_request"), &keys.review_request)
        .output_set(&state_key("answer"), &keys.answer)
        .output_set(&state_key("is_validated"), &keys.validated)
        .output_set(&state_key("nb_generations"), &keys.generations)
        .output_set(&state_key("thread"), &keys.thread)
        .end_label_set(&keys.outcome);
    let id = NodeId::new("duo").expect("node id");
    let graph = GraphBuilder::new(schema.clone())
        .entry(id.clone())
        .subgraph(id.clone(), call)
        .edge(
            id,
            Always(Target::End(EndLabel::new("done").expect("label"))),
        )
        .input(&keys.question)
        .input(&keys.thread)
        .build()
        .expect("build");

    let state = graph
        .start_state([
            keys.question.given("q1").expect("question"),
            keys.thread.given("").expect("a new thread"),
        ])
        .expect("start state");
    let config = Config::new(
        &schema,
        [(
            keys.review_request.key().clone(),
            Value::str("Does the answer above leave out a key fact?"),
        )]
        .into(),
    )
    .expect("config");

    let (_sender, mut channel) = control();
    match run(&graph, &config, state, None, &quiet_context(), &mut channel).await {
        Ok(Outcome::Finished { state, .. }) => report(&keys, &state, &store),
        Ok(_) => println!("generator_critic paused or cancelled"),
        Err(failure) => println!("generator_critic failed: {}", failure.error),
    }
}

/// The generator-critic: inputs `opening`, `question_id` and `thread`;
/// configuration `review_request`.
fn generator_critic(store: Arc<MemoryStore>) -> Arc<Graph> {
    // The generator's system text reads `question_id`, declared with it: an
    // input of the graph.
    let mut reads = Reads::new();
    let question_id: StateKey<String> = reads.state("question_id").expect("key");
    let read = question_id.clone();
    let generator = GeneratorSeat {
        author: Author::new("writer", "writer").expect("author"),
        model: Model::client(scripted(vec![
            text_step("Rounds of nodes."),
            text_step("Supersteps run the active nodes in rounds."),
            text_step(
                "Supersteps run the active nodes in parallel rounds, then apply every update at once.",
            ),
        ])),
        system: Prompt::from_fn(reads, move |ctx| {
            Ok(format!("Answer question {} precisely.", ctx.get(&read)?))
        }),
        history: None,
        output_mode: OutputMode::Plain,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        reply_retries: None,
        reminder: None,
    };
    let critic = CriticSeat {
        review_request: Some(Prompt::from_config(&config_key("review_request"))),
        ..CriticSeat::new(
            Author::new("reviewer", "reviewer").expect("author"),
            Model::client(Arc::new(scripted_at(
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
                        "message": "Clear and complete."
                    })),
                ],
            ))),
            Prompt::text("Reject an answer that leaves out a key fact."),
        )
    };
    // Code judges each answer before the critic: a short one is revised.
    let rule_check = RuleCheck::from_fn(Reads::new(), |ctx| {
        let words: usize = ctx
            .answer_step()
            .text()
            .map(|text| text.as_str().split_whitespace().count())
            .sum();
        Ok(if words < 5 {
            Ruling::Revise(Text::new("Write a full sentence.")?)
        } else {
            Ruling::Pass
        })
    });
    let bank = Arc::new(QuestionBank(BTreeMap::from([(
        "q1",
        "Explain supersteps.",
    )])));
    GeneratorCritic {
        rule_check: Some(rule_check),
        prepare: Some(Box::new(OpenQuestion {
            bank,
            question_id,
            opening: state_key("opening"),
        })),
        thread_store: Some(store),
        ..GeneratorCritic::new(generator, critic, Count::Fixed(3))
    }
    .graph()
    .expect("generator-critic graph")
}

fn report(keys: &Keys, state: &State, store: &MemoryStore) {
    println!(
        "generator_critic ended {} after {} generations (validated: {})",
        state.get(&keys.outcome).expect("outcome"),
        state.get(&keys.generations).expect("generations"),
        state.get(&keys.validated).expect("validated"),
    );
    println!("  answer: {}", state.get(&keys.answer).expect("answer"));
    store.print(&ThreadId::new(state.get(&keys.thread).expect("thread")).expect("a thread id"));
}
