//! What the hook tests share: a host's hook judging as told, a tool it
//! guards, a gauge of what runs at once, and a tool node on a pending turn.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_llm_clients_core::ToolSpec;
use br_llm_messages::{
    AssistantBlock, Conversation, Entry, Step, StopReason, Text, ToolCall, ToolCallId, ToolName,
    ToolResultBlock, Turn, TurnId, UserBlock, UserInput, UserSource,
};
use serde_json::{Value as Json, json};

use crate::graph::{Always, Context, Graph, GraphBuilder, Limit, Target};
use crate::observe::NoopObserver;
use crate::react::hook::{Hook, HookFuture, Judgment};
use crate::react::test_support::*;
use crate::react::tool::{Tool, ToolFuture, ToolOutput};
use crate::react::tool_gate::SharedGates;
use crate::react::tool_node::ToolNode;
use crate::run::{Outcome, RunFailure, channel, run};
use crate::state::{Config, State, Value};
use crate::testkit::SeqIds;
use crate::value::{EndLabel, NodeId};

/// How many hooks and tools of the test run at once, and the most at once.
#[derive(Default)]
pub(crate) struct Gauge {
    live: AtomicUsize,
    peak: AtomicUsize,
}

impl Gauge {
    pub(crate) fn enter(&self) {
        let now = self.live.fetch_add(1, Ordering::SeqCst) + 1;
        self.peak.fetch_max(now, Ordering::SeqCst);
    }

    pub(crate) fn leave(&self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }

    pub(crate) fn peak(&self) -> usize {
        self.peak.load(Ordering::SeqCst)
    }
}

/// What a test hook does with each call it judges.
#[derive(Clone)]
pub(crate) enum Behaviour {
    Accept,
    Refuse(&'static str),
    Fail(&'static str),
    /// Accepts the call after `yields` turns of the executor, refuses it
    /// when its arguments say `{"refuse": true}`.
    Slow {
        yields: usize,
    },
    /// Fails every call after `yields` turns of the executor, with
    /// `PolicyDown(name)`.
    SlowFail {
        yields: usize,
        name: &'static str,
    },
    /// Fails its first judgment after `yields` turns of the executor, with
    /// `PolicyDown(name)`, and accepts every later one: a policy service
    /// down once.
    SlowFailOnce {
        yields: usize,
        name: &'static str,
    },
}

/// What a hook was called with: the call's id, its arguments, the length of
/// the state key `log` and the configuration key `base`.
pub(crate) type Seen = (String, Json, String, String);

/// A host's hook: judges each call as its behaviour says, keeps what it was
/// called with, and takes the gauge from its start (the tool leaves it).
pub(crate) struct TestHook {
    behaviour: Behaviour,
    gauge: Arc<Gauge>,
    seen: Mutex<Vec<Seen>>,
}

/// The error of a failing hook: a type of the host's.
#[derive(Debug)]
pub(crate) struct PolicyDown(pub(crate) &'static str);

impl std::fmt::Display for PolicyDown {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "policy service down: {}", self.0)
    }
}

impl std::error::Error for PolicyDown {}

impl Hook for TestHook {
    fn judge<'a>(
        &'a self,
        id: &'a ToolCallId,
        arguments: &'a Json,
        state: &'a State,
        config: &'a Config,
    ) -> HookFuture<'a> {
        Box::pin(async move {
            self.gauge.enter();
            let scope = state.list(&key("log"))?.len().to_string();
            let base = config.str(&key("base"))?.to_owned();
            let judged = {
                let mut seen = self.seen.lock().unwrap();
                seen.push((id.as_str().to_owned(), arguments.clone(), scope, base));
                seen.len()
            };
            let judgment = match &self.behaviour {
                Behaviour::Accept => Judgment::Accept,
                Behaviour::Refuse(reason) => Judgment::Refuse {
                    reason: (*reason).to_owned(),
                },
                Behaviour::Fail(name) if arguments["name"] == **name => {
                    self.gauge.leave();
                    return Err(PolicyDown(name).into());
                }
                Behaviour::Fail(_) => Judgment::Accept,
                Behaviour::SlowFail { yields, name } => {
                    for _ in 0..*yields {
                        tokio::task::yield_now().await;
                    }
                    self.gauge.leave();
                    return Err(PolicyDown(name).into());
                }
                Behaviour::SlowFailOnce { yields, name } => {
                    for _ in 0..*yields {
                        tokio::task::yield_now().await;
                    }
                    if judged == 1 {
                        self.gauge.leave();
                        return Err(PolicyDown(name).into());
                    }
                    Judgment::Accept
                }
                Behaviour::Slow { yields } => {
                    for _ in 0..*yields {
                        tokio::task::yield_now().await;
                    }
                    if arguments["refuse"] == json!(true) {
                        Judgment::Refuse {
                            reason: "refused slowly".to_owned(),
                        }
                    } else {
                        Judgment::Accept
                    }
                }
            };
            if !matches!(judgment, Judgment::Accept) {
                self.gauge.leave();
            }
            Ok(judgment)
        })
    }
}

/// A tool counting the calls it starts and those it ends, guarded by `hook`
/// when it has one. A call with `{"yields": n}` takes n turns of the
/// executor.
pub(crate) struct GuardedTool {
    spec: ToolSpec,
    hook: Option<TestHook>,
    gauge: Arc<Gauge>,
    calls: AtomicUsize,
    finished: AtomicUsize,
}

impl GuardedTool {
    pub(crate) fn new(name: &str, behaviour: Option<Behaviour>, gauge: &Arc<Gauge>) -> Arc<Self> {
        Arc::new(Self {
            spec: spec(name),
            hook: behaviour.map(|behaviour| TestHook {
                behaviour,
                gauge: gauge.clone(),
                seen: Mutex::new(Vec::new()),
            }),
            gauge: gauge.clone(),
            calls: AtomicUsize::new(0),
            finished: AtomicUsize::new(0),
        })
    }

    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// The calls whose tool ran to its end.
    pub(crate) fn finished(&self) -> usize {
        self.finished.load(Ordering::SeqCst)
    }

    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.hook
            .as_ref()
            .map(|hook| hook.seen.lock().unwrap().clone())
            .unwrap_or_default()
    }
}

impl Tool for GuardedTool {
    fn spec(&self) -> &ToolSpec {
        &self.spec
    }

    fn hook(&self) -> Option<&dyn Hook> {
        self.hook.as_ref().map(|hook| hook as &dyn Hook)
    }

    fn call<'a>(
        &'a self,
        arguments: Json,
        _state: &'a State,
        _config: &'a Config,
    ) -> ToolFuture<'a> {
        Box::pin(async move {
            if self.hook.is_none() {
                self.gauge.enter();
            }
            self.calls.fetch_add(1, Ordering::SeqCst);
            for _ in 0..arguments["yields"].as_u64().unwrap_or(0) {
                tokio::task::yield_now().await;
            }
            self.gauge.leave();
            self.finished.fetch_add(1, Ordering::SeqCst);
            Ok(ToolOutput::text(vec![ToolResultBlock::text(Text::new(
                format!("{} ran", self.spec.name()),
            )?)]))
        })
    }
}

pub(crate) fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

pub(crate) fn call(id: &str, name: &str, arguments: Json) -> AssistantBlock {
    AssistantBlock::ToolCall(ToolCall {
        id: ToolCallId::new(id).unwrap(),
        name: ToolName::new(name).unwrap(),
        arguments,
    })
}

/// The agent's turn on `chat` awaiting the results of `calls`.
pub(crate) fn pending(calls: Vec<AssistantBlock>) -> State {
    let step = Step::new(calls, StopReason::AwaitingToolResults, None, None).unwrap();
    let mut chat = Conversation::new();
    chat.push_input(
        UserInput::new(
            UserSource::Human,
            None,
            vec![UserBlock::text(Text::new("go").unwrap())],
        )
        .unwrap(),
    );
    chat.push_turn(Turn::new(TurnId::new("t0").unwrap(), Some(author()), step))
        .unwrap();
    let values = BTreeMap::from([
        (key("chat"), Value::conversation(chat)),
        (key("log"), Value::list(vec![Value::str("one entry")])),
        (key("enabled"), Value::list(Vec::new())),
    ]);
    State::new(schema(), values).unwrap()
}

/// A graph of one tool node running `tools`, ending `done`.
pub(crate) fn tool_graph(tools: Vec<Arc<dyn Tool>>, width: Option<usize>, enabled: bool) -> Graph {
    let node = ToolNode {
        key: key("chat"),
        author: author(),
        tools,
        max_concurrency: width.map(|width| Limit::Fixed(NonZeroUsize::new(width).unwrap())),
        enabled: enabled.then(|| key("enabled")),
        gates: Arc::new(SharedGates::default()),
    };
    GraphBuilder::new(schema())
        .entry(nid("tools"))
        .node(nid("tools"), node)
        .edge(
            nid("tools"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap()
}

pub(crate) async fn run_tools(graph: &Graph, state: State) -> Result<Outcome, RunFailure> {
    let ctx = Context::new(Arc::new(NoopObserver), Arc::new(SeqIds::new()));
    let (_sender, mut inbox) = channel();
    run(graph, &config(), state, None, &ctx, &mut inbox).await
}

/// The tool results of the agent's turn, as `(tool, is_error, text)`.
pub(crate) fn answered(result: Result<Outcome, RunFailure>) -> Vec<(String, bool, String)> {
    let Outcome::Finished { state, .. } = result.map_err(|failure| failure.error).unwrap() else {
        panic!("expected finished");
    };
    let Some(Entry::Turn(turn)) = state.conversation(&key("chat")).unwrap().entries().last() else {
        panic!("expected the agent's turn");
    };
    results_of(turn)
}

pub(crate) fn ran(name: &str) -> (String, bool, String) {
    (name.to_owned(), false, format!("{name} ran"))
}

pub(crate) fn refused(name: &str, reason: &str) -> (String, bool, String) {
    (name.to_owned(), true, reason.to_owned())
}
