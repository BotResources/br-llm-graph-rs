//! What the ReAct agent tests share: the agent on a mock client, its
//! configuration, and the calls that run it.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_llm_clients_core::{OutputMode, Parameter, ParameterKind, Parameters, StructuredShape};
use br_llm_clients_mock::MockClient;
use br_llm_messages::{
    Conversation, Entry, InputId, Step, Text, Turn, TurnItem, UserBlock, UserInput, UserSource,
};
use serde_json::Value as Json;

use crate::graph::{Context, Count, Graph, Limit};
use crate::observe::NoopObserver;
use crate::react::given::Model;
use crate::react::react_agent::ReactAgent;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;
use crate::run::{Outcome, RunFailure, control, run};
use crate::state::{Config, State, Value};
use crate::testkit::{SeqIds, Transcript};
use crate::value::NodeId;

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
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

pub(crate) fn agent(client: &MockClient) -> ReactAgent {
    ReactAgent {
        author: author(),
        model: Model::client(Arc::new(client.clone())),
        system: base(),
        history: None,
        output_mode: OutputMode::Plain,
        tools: vec![Arc::new(EchoTool)],
        tool_nodes: Vec::new(),
        tool_concurrency: None,
        round_limit: None,
        enabled: None,
        reply_retries: None,
        reminder: None,
        inputs: Vec::new(),
        thread_store: None,
    }
}

pub(crate) fn limited(client: &MockClient, max_rounds: usize, on_limit: OnLimit) -> ReactAgent {
    ReactAgent {
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(max_rounds).unwrap()),
            on_limit,
        }),
        ..agent(client)
    }
}

pub(crate) fn agent_config(graph: &Graph) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("base"), Value::str("You are a helpful agent."));
    Config::new(graph.schema(), values).unwrap()
}

pub(crate) fn ctx() -> Context {
    Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()))
}

/// What a call of the agent gave: its result, and its private conversation
/// as the observer saw it built.
pub(crate) struct Called {
    pub(crate) result: Result<Outcome, RunFailure>,
    transcript: Option<Arc<Transcript>>,
}

impl Called {
    /// The result of a run whose conversation `transcript` kept.
    pub(crate) fn new(result: Result<Outcome, RunFailure>, transcript: Arc<Transcript>) -> Self {
        Self {
            result,
            transcript: Some(transcript),
        }
    }

    pub(crate) fn err(self) -> Option<RunFailure> {
        self.result.err()
    }
}

impl From<Result<Outcome, RunFailure>> for Called {
    fn from(result: Result<Outcome, RunFailure>) -> Self {
        Self {
            result,
            transcript: None,
        }
    }
}

/// A finished call: its final state and, when seen, its conversation.
pub(crate) struct Done {
    pub(crate) state: State,
    conversation: Option<Conversation>,
}

impl Done {
    /// The agent's conversation as the run left it.
    pub(crate) fn transcript(&self) -> &Conversation {
        self.conversation
            .as_ref()
            .expect("the run built a conversation")
    }
}

impl std::ops::Deref for Done {
    type Target = State;

    fn deref(&self) -> &State {
        &self.state
    }
}

pub(crate) async fn call(graph: &Graph, opening: Conversation) -> Called {
    call_in(graph, opening, &ctx()).await
}

/// `ctx` with an observer that also keeps the agent's conversation.
pub(crate) fn observed(ctx: &Context) -> (Context, Arc<Transcript>) {
    let transcript = Arc::new(Transcript::new(ctx.observer.clone()));
    let mut ctx = ctx.clone();
    ctx.observer = transcript.clone();
    (ctx, transcript)
}

pub(crate) async fn run_agent(graph: &Graph, state: State) -> Called {
    let (ctx, transcript) = observed(&ctx());
    let (_sender, mut channel) = control();
    let result = run(graph, &agent_config(graph), state, None, &ctx, &mut channel).await;
    Called {
        result,
        transcript: Some(transcript),
    }
}

pub(crate) async fn call_in(graph: &Graph, opening: Conversation, ctx: &Context) -> Called {
    let state = graph
        .start_state([(key("opening"), Value::conversation(opening))])
        .unwrap();
    let (ctx, transcript) = observed(ctx);
    let (_sender, mut channel) = control();
    let result = run(graph, &agent_config(graph), state, None, &ctx, &mut channel).await;
    Called {
        result,
        transcript: Some(transcript),
    }
}

pub(crate) fn finished(called: impl Into<Called>) -> Done {
    let called = called.into();
    let conversation = called
        .transcript
        .as_ref()
        .and_then(|transcript| transcript.conversation());
    match called.result.map_err(|f| f.error).unwrap() {
        Outcome::Finished { state, end } => {
            assert_eq!(end.as_str(), "done");
            Done {
                state,
                conversation,
            }
        }
        Outcome::Paused { .. } | Outcome::Cancelled { .. } => panic!("expected finished"),
    }
}

/// The agent's last turn: the last entry of its conversation.
pub(crate) fn last_turn(done: &Done) -> Turn {
    match done.transcript().entries().last() {
        Some(Entry::Turn(turn)) => turn.clone(),
        Some(Entry::UserInput(_)) | None => panic!("expected the agent's turn last"),
    }
}

/// The shape `found`: `answer` (string), required.
pub(crate) fn found_shape() -> OutputMode {
    let parameters = Parameters::new(vec![
        Parameter::new("answer", "", true, ParameterKind::String).unwrap(),
    ])
    .unwrap();
    OutputMode::Structured(StructuredShape::new("found", None, parameters).unwrap())
}

/// The agent on `client`, asking for `found`, with `retries`.
pub(crate) fn asking_found(client: &MockClient, retries: Option<Count>) -> ReactAgent {
    ReactAgent {
        output_mode: found_shape(),
        reply_retries: retries,
        ..agent(client)
    }
}

/// The steps of the agent's last turn, tool results left out.
pub(crate) fn last_steps(done: &Done) -> Vec<Step> {
    last_turn(done)
        .items()
        .iter()
        .filter_map(|item| match item {
            TurnItem::Step(step) => Some(step.clone()),
            TurnItem::ToolResults(_) => None,
        })
        .collect()
}

/// The structured blocks of `steps`, in order.
pub(crate) fn blocks_of(steps: &[Step]) -> Vec<Json> {
    steps
        .iter()
        .flat_map(|step| step.structured().cloned())
        .collect()
}

/// The reminder the agent sends by default after a reply refused for
/// `reason`.
pub(crate) fn reminder_of(reason: &str) -> br_llm_messages::WireMessage {
    crate::react::generator_critic_view_support::user_message(&format!(
        "Your reply could not be used: {reason}. Reply again."
    ))
}
