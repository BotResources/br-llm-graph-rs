//! What the hook tests share: a host's hook judging as told, a tool it
//! guards, and a gauge of what runs at once; the tool node they run in is
//! in `react_hook_node_support`, re-exported here.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use br_llm_clients_core::ToolSpec;
use br_llm_messages::{Text, ToolCallId, ToolResultBlock};
use serde_json::{Value as Json, json};

use crate::react::hook::{Hook, HookFuture, Judgment};
pub(crate) use crate::react::react_hook_node_support::*;
use crate::react::test_support::*;
use crate::react::tool::{Tool, ToolContext, ToolFuture, ToolOutput};
use crate::state::{Config, State};

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
    /// Panics while it judges.
    Panic,
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
                Behaviour::Panic => panic!("the policy hook broke"),
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
        _call: &'a ToolContext,
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
