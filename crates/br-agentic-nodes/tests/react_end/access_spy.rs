use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use br_agentic_contract::memory::MemoryThreadAccess;
use br_agentic_contract::{PortError, PortFuture, ResultStream, StreamSink};
use br_llm_clients_core::BoxFuture;
use br_llm_messages::{AgentId, ClosureReason, Conversation, ResponseDelta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Read,
    AddInputTurn,
    AddEntry,
    CloseTurn,
    OpenResponse,
    CompleteResponse,
    DiscardResponse,
    OpenResult,
    CompleteResult,
    DiscardResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Fail,
    Hang,
}

#[derive(Debug, Clone, Copy)]
pub struct Rule {
    pub op: Op,
    pub action: Action,
    pub agent: Option<AgentId>,
    pub nth: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Hand {
    pub op: Op,
    pub agent: Option<AgentId>,
    pub reason: Option<ClosureReason>,
    pub started: tokio::time::Instant,
}

pub(crate) struct NullSink;

impl StreamSink for NullSink {
    fn send<'a>(&'a mut self, _delta: ResponseDelta) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

pub(crate) struct NullResult;

impl ResultStream for NullResult {
    fn send<'a>(&'a mut self, _fragment: String) -> PortFuture<'a, ()> {
        Box::pin(async {})
    }
}

pub struct AccessSpy {
    pub(crate) inner: Arc<MemoryThreadAccess>,
    pub(crate) kept: Option<Conversation>,
    rules: Vec<Rule>,
    delay: Duration,
    slow_storage: Option<Duration>,
    log: Mutex<Vec<Hand>>,
    in_flight: AtomicUsize,
    max_in_flight: AtomicUsize,
}

impl AccessSpy {
    pub fn over(inner: Arc<MemoryThreadAccess>) -> Self {
        Self {
            inner,
            kept: None,
            rules: Vec::new(),
            delay: Duration::ZERO,
            slow_storage: None,
            log: Mutex::new(Vec::new()),
            in_flight: AtomicUsize::new(0),
            max_in_flight: AtomicUsize::new(0),
        }
    }

    pub fn forgetful(read: Conversation, inner: Arc<MemoryThreadAccess>) -> Self {
        let mut spy = Self::over(inner);
        spy.kept = Some(read);
        spy
    }

    pub fn failing(mut self, op: Op) -> Self {
        self.rules.push(Rule {
            op,
            action: Action::Fail,
            agent: None,
            nth: None,
        });
        self
    }

    pub fn hanging(mut self, op: Op) -> Self {
        self.rules.push(Rule {
            op,
            action: Action::Hang,
            agent: None,
            nth: None,
        });
        self
    }

    pub fn with_rule(mut self, rule: Rule) -> Self {
        self.rules.push(rule);
        self
    }

    pub fn slow(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_slow_storage(mut self, storage: Duration) -> Self {
        self.slow_storage = Some(storage);
        self
    }

    pub fn log(&self) -> Vec<Hand> {
        self.log.lock().unwrap().clone()
    }

    pub fn ops(&self) -> Vec<Op> {
        self.log().iter().map(|hand| hand.op).collect()
    }

    pub fn count(&self, op: Op) -> usize {
        self.ops().into_iter().filter(|seen| *seen == op).count()
    }

    pub fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(Ordering::SeqCst)
    }

    pub fn closures(&self, agent: AgentId) -> Vec<ClosureReason> {
        self.log()
            .into_iter()
            .filter(|hand| hand.op == Op::CloseTurn && hand.agent == Some(agent))
            .filter_map(|hand| hand.reason)
            .collect()
    }

    pub(crate) async fn gate(
        &self,
        op: Op,
        agent: Option<AgentId>,
        reason: Option<ClosureReason>,
    ) -> Result<(), PortError> {
        let nth = {
            let mut log = self.log.lock().unwrap();
            let nth = log.iter().filter(|hand| hand.op == op).count();
            log.push(Hand {
                op,
                agent,
                reason,
                started: tokio::time::Instant::now(),
            });
            nth
        };
        let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.max_in_flight.fetch_max(now, Ordering::SeqCst);
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        let rule = self.rules.iter().find(|rule| {
            rule.op == op
                && rule.agent.is_none_or(|wanted| Some(wanted) == agent)
                && rule.nth.is_none_or(|wanted| wanted == nth)
        });
        let outcome = match rule.map(|rule| rule.action) {
            Some(Action::Fail) => Err(PortError::fault(format!("{op:?} failed"))),
            Some(Action::Hang) => std::future::pending().await,
            None => Ok(()),
        };
        if let Some(storage) = self.slow_storage {
            tokio::spawn(async move { tokio::time::sleep(storage).await });
        }
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        outcome
    }

    pub(crate) fn forgets(&self) -> bool {
        self.kept.is_some()
    }
}
