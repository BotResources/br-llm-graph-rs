//! What the generator-critic tests share: the two seats on mock clients, the
//! calls, and a compact view of the histories.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_clients_mock::MockClient;
use br_llm_messages::{
    AssistantBlock, Author, Conversation, Entry, Step, StopReason, Text, Turn, TurnItem, UserBlock,
    UserInput, UserSource, WireMessage, WireUserBlock,
};
use serde_json::json;

use crate::error::{GraphError, NodeFault};
use crate::graph::{Context, Count, Graph, Limit};
use crate::observe::NoopObserver;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::source::Source;
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, State, Value};
use crate::testkit::SeqIds;
use crate::value::{Key, NodeId};

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn writer() -> Author {
    Author::new("writer", "writer").unwrap()
}

pub(crate) fn reviewer() -> Author {
    Author::new("reviewer", "reviewer").unwrap()
}

pub(crate) fn limit(value: usize) -> Limit {
    Limit::Fixed(NonZeroUsize::new(value).unwrap())
}

pub(crate) fn question(body: &str) -> Conversation {
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new(body).unwrap())],
        )
        .unwrap(),
    );
    chat
}

/// A structured verdict of the default shape.
pub(crate) fn verdict(is_valid: bool, message: &str) -> Step {
    structured_step(json!({ "thinking": "weighing it", "is_valid": is_valid, "message": message }))
}

/// The generator seat on `client`, without tools.
pub(crate) fn generator(client: &MockClient) -> GeneratorSeat {
    GeneratorSeat {
        author: writer(),
        client: Arc::new(client.clone()),
        system: Source::Const("Answer the question.".to_owned()),
        output: OutputMode::Plain,
        tools: Vec::new(),
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
    }
}

/// The critic seat on `client`, with the defaults.
pub(crate) fn critic(client: &MockClient) -> CriticSeat {
    CriticSeat::new(
        reviewer(),
        Arc::new(client.clone()),
        Source::Const("Judge the answer.".to_owned()),
    )
}

/// A generator-critic with a budget of three critiques.
pub(crate) fn duo(generator_client: &MockClient, critic_client: &MockClient) -> GeneratorCritic {
    GeneratorCritic::new(
        generator(generator_client),
        critic(critic_client),
        Count::Fixed(3),
    )
}

/// A generator with `echo` and one tool round, cut by `OnLimit::End`.
pub(crate) fn cut_generator(client: &MockClient) -> GeneratorSeat {
    GeneratorSeat {
        tools: vec![Arc::new(EchoTool)],
        round_limit: Some(RoundLimit {
            max_rounds: limit(1),
            on_limit: OnLimit::End {
                node: nid("limit"),
                flag: key("limited"),
            },
        }),
        ..generator(client)
    }
}

/// The closing step's text of a generation cut by `cut_generator`'s limit.
pub(crate) const CUT: &str = "Tool round limit reached (1): no final reply.";

/// The default critique of an empty answer.
pub(crate) const EMPTY: &str = "The answer was empty. Write a complete answer.";

pub(crate) fn empty_critique() -> String {
    format!("reviewer:{EMPTY}")
}

pub(crate) fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

pub(crate) async fn call_with(
    graph: &Graph,
    conversation: Conversation,
    generator_history: Conversation,
    config: BTreeMap<Key, Value>,
) -> Result<Outcome, RunFailure> {
    let state = graph
        .start_state([
            (key("conversation"), Value::conversation(conversation)),
            (
                key("generator_history"),
                Value::conversation(generator_history),
            ),
        ])
        .unwrap();
    let config = Config::new(graph.schema(), config).unwrap();
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx(), &mut inbox).await
}

/// A new call on `question("explain")` with the given extra inputs.
pub(crate) async fn start(graph: &Graph, extra: Vec<(Key, Value)>) -> Result<Outcome, RunFailure> {
    let mut inputs = vec![
        (
            key("conversation"),
            Value::conversation(question("explain")),
        ),
        (
            key("generator_history"),
            Value::conversation(Conversation::new()),
        ),
    ];
    inputs.extend(extra);
    let state = graph.start_state(inputs).unwrap();
    let config = Config::new(graph.schema(), BTreeMap::new()).unwrap();
    let (_sender, mut inbox) = channel();
    run(graph, &config, state, None, &ctx(), &mut inbox).await
}

pub(crate) async fn call(graph: &Graph, conversation: Conversation) -> Result<Outcome, RunFailure> {
    call_with(graph, conversation, Conversation::new(), BTreeMap::new()).await
}

pub(crate) fn ended(result: Result<Outcome, RunFailure>, label: &str) -> State {
    match result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, end } => {
            assert_eq!(end.as_str(), label);
            state
        }
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

/// The checkpoint state of a run that failed in `node` with an error that
/// node returned, and that error.
pub(crate) fn failed_in(result: Result<Outcome, RunFailure>, node: &str) -> (State, GraphError) {
    let failure = result.err().unwrap();
    match failure.error {
        GraphError::NodeFailed {
            node: failed,
            source: NodeFault::Returned(error),
        } => {
            assert_eq!(failed, nid(node));
            (
                failure.checkpoint.state,
                *error.downcast::<GraphError>().unwrap(),
            )
        }
        other => panic!("expected node {node} to fail, got: {other}"),
    }
}

/// Each entry as `user`, or `author:text` with the text of the turn's final
/// step, or `author:[n items]` for a turn of several items.
pub(crate) fn shape(conversation: &Conversation) -> Vec<String> {
    conversation
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::UserInput(_) => "user".to_owned(),
            Entry::Turn(turn) if turn.items().len() > 1 => {
                format!("{}:[{} items]", turn.author().unwrap(), turn.items().len())
            }
            Entry::Turn(turn) => format!("{}:{}", turn.author().unwrap(), turn_text(turn)),
        })
        .collect()
}

fn turn_text(turn: &Turn) -> String {
    match turn.items().last() {
        Some(TurnItem::Step(step)) => step.text().map(Text::as_str).collect::<Vec<_>>().join(" "),
        Some(TurnItem::ToolResults(_)) | None => String::new(),
    }
}

pub(crate) fn shape_of(state: &State, name: &str) -> Vec<String> {
    shape(state.conversation(&key(name)).unwrap())
}

pub(crate) fn user_message(text: &str) -> WireMessage {
    WireMessage::User {
        content: vec![WireUserBlock::Text {
            text: Text::new(text).unwrap(),
        }],
    }
}

pub(crate) fn assistant_message(blocks: Vec<AssistantBlock>) -> WireMessage {
    WireMessage::Assistant { content: blocks }
}

pub(crate) fn text_block(text: &str) -> AssistantBlock {
    AssistantBlock::Text {
        text: Text::new(text).unwrap(),
    }
}

pub(crate) fn thinking_block(text: &str) -> AssistantBlock {
    AssistantBlock::Thinking(br_llm_messages::Thinking {
        text: text.to_owned(),
        signature: None,
    })
}

/// An answer whose reasoning comes before its text.
pub(crate) fn thought_step(thinking: &str, text: &str) -> Step {
    Step::new(
        vec![thinking_block(thinking), text_block(text)],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap()
}

/// A generator-critic on the two clients, with `max_critiques` and
/// `final_generation`.
pub(crate) fn with_budget(
    generator_client: &MockClient,
    critic_client: &MockClient,
    max_critiques: Count,
    final_generation: bool,
) -> Arc<Graph> {
    GeneratorCritic {
        final_generation,
        ..GeneratorCritic::new(
            generator(generator_client),
            critic(critic_client),
            max_critiques,
        )
    }
    .graph()
    .unwrap()
}
