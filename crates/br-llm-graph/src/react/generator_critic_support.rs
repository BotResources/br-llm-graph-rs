//! What the generator-critic tests share: the two seats on mock clients, the
//! calls, and a compact view of the conversation.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::OutputMode;
use br_llm_clients_mock::MockClient;
use br_llm_messages::{
    AssistantBlock, Author, Conversation, Entry, InputId, Step, StopReason, Text, Turn, TurnItem,
    UserBlock, UserInput, UserSource, WireMessage, WireUserBlock,
};
use serde_json::json;

use crate::error::{GraphError, NodeFault};
use crate::graph::{Context, Count, Graph, Limit};
use crate::observe::NoopObserver;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::given::{Model, Prompt};
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::seats::{CriticSeat, GeneratorSeat};
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, control, run};
use crate::state::{Config, State, Value};
use crate::testkit::{SeqIds, Transcript};
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
            InputId::new(format!("i-{body}")).unwrap(),
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new(body).unwrap())],
        )
        .unwrap(),
    )
    .unwrap();
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
        model: Model::client(Arc::new(client.clone())),
        system: Prompt::text("Answer the question."),
        history: None,
        output_mode: OutputMode::Plain,
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
        Model::client(Arc::new(client.clone())),
        Prompt::text("Judge the answer."),
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

/// What a call of the generator-critic gave: its result, and its private
/// conversation as the observer saw it built.
pub(crate) struct Ran {
    pub(crate) result: Result<Outcome, RunFailure>,
    transcript: Arc<Transcript>,
}

impl Ran {
    /// The conversation as the run left it (empty when it built none).
    pub(crate) fn transcript(&self) -> Conversation {
        self.transcript.conversation().unwrap_or_default()
    }
}

/// A finished call: its final state and its conversation.
pub(crate) struct Done {
    pub(crate) state: State,
    conversation: Conversation,
}

impl Done {
    /// The generator-critic's conversation as the run left it.
    pub(crate) fn transcript(&self) -> &Conversation {
        &self.conversation
    }
}

impl std::ops::Deref for Done {
    type Target = State;

    fn deref(&self) -> &State {
        &self.state
    }
}

/// Runs `graph` on `state` with `config` in `ctx`, keeping its conversation.
pub(crate) async fn run_in(graph: &Graph, state: State, config: &Config, ctx: &Context) -> Ran {
    let transcript = Arc::new(Transcript::new(ctx.observer.clone()));
    let mut ctx = ctx.clone();
    ctx.observer = transcript.clone();
    let (_sender, mut channel) = control();
    let result = run(graph, config, state, None, &ctx, &mut channel).await;
    Ran { result, transcript }
}

/// A call on `opening` with the extra inputs `extra` and the configuration
/// `config`.
pub(crate) async fn call_with(
    graph: &Graph,
    opening: Conversation,
    extra: Vec<(Key, Value)>,
    config: BTreeMap<Key, Value>,
) -> Ran {
    let mut inputs = vec![(key("opening"), Value::conversation(opening))];
    inputs.extend(extra);
    let state = graph.start_state(inputs).unwrap();
    let config = Config::new(graph.schema(), config).unwrap();
    run_in(graph, state, &config, &ctx()).await
}

/// A new call on `question("explain")` with the given extra inputs.
pub(crate) async fn start(graph: &Graph, extra: Vec<(Key, Value)>) -> Ran {
    call_with(graph, question("explain"), extra, BTreeMap::new()).await
}

pub(crate) async fn call(graph: &Graph, opening: Conversation) -> Ran {
    call_with(graph, opening, Vec::new(), BTreeMap::new()).await
}

pub(crate) fn ended(ran: Ran, label: &str) -> Done {
    let conversation = ran.transcript();
    match ran.result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, end } => {
            assert_eq!(end.as_str(), label);
            Done {
                state,
                conversation,
            }
        }
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

/// The conversation of a run that failed in `node` with an error that node
/// returned, and that error.
pub(crate) fn failed_in(ran: Ran, node: &str) -> (Conversation, GraphError) {
    let conversation = ran.transcript();
    let failure = ran.result.err().unwrap();
    match failure.error {
        GraphError::NodeFailed {
            node: failed,
            source: NodeFault::Returned(error),
        } => {
            assert_eq!(failed, nid(node));
            (conversation, *error.downcast::<GraphError>().unwrap())
        }
        other => panic!("expected node {node} to fail, got: {other}"),
    }
}

/// Each entry as `user`, or `author:text` with the text of the turn's final
/// step, or `author:[n items] text` for a turn of several items.
pub(crate) fn shape(conversation: &Conversation) -> Vec<String> {
    conversation
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::UserInput(_) => "user".to_owned(),
            Entry::Turn(turn) if turn.items().len() > 1 => {
                format!(
                    "{}:[{} items] {}",
                    turn.author().unwrap(),
                    turn.items().len(),
                    turn_text(turn)
                )
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
