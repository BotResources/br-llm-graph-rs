//! The tool results of one round of a durable agent, written to its thread
//! as one item: every tool node of the ReAct loop runs in the round's
//! superstep, and the round's item is pushed whole once all of them are
//! done, before any of them gives back its updates.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll, Waker};

use br_llm_messages::ToolResult;

use crate::error::GraphError;
use crate::state::State;

/// What a tool node of the round gives: its results in call order, or
/// nothing when it failed (its error, its panic or its cancel), in which case
/// the round is not written and the superstep fails.
enum Deposit {
    Results(Vec<ToolResult>),
    Failed,
}

/// How a round ended: written, not written because a tool node failed, or
/// refused by the store (its error, shared by every tool node).
#[derive(Clone)]
enum Outcome {
    Written,
    NotWritten,
    StoreFailed(Arc<dyn std::error::Error + Send + Sync>),
}

#[derive(Default)]
struct Round {
    /// One place per tool node, in registration order.
    deposits: Vec<Option<Deposit>>,
    outcome: Option<Outcome>,
    wakers: Vec<Waker>,
    /// The tool nodes holding a seat.
    holders: usize,
}

/// The rounds of the tool nodes of one ReAct loop under way: one round per
/// superstep, told by the state its nodes read, as the tool concurrency
/// gates are, so concurrent runs of one graph never share a round.
#[derive(Default)]
pub(crate) struct RoundWriters {
    rounds: Mutex<HashMap<usize, Round>>,
}

impl RoundWriters {
    /// The seat of tool node `node` (0-based, of `nodes`) in the round of
    /// the superstep reading `state`.
    pub(crate) fn seat(self: &Arc<Self>, state: &State, nodes: usize, node: usize) -> Seat {
        let superstep = std::ptr::from_ref(state) as usize;
        let mut rounds = self.lock();
        let round = rounds.entry(superstep).or_default();
        if round.deposits.len() < nodes {
            round.deposits.resize_with(nodes, || None);
        }
        round.holders += 1;
        Seat {
            writers: self.clone(),
            superstep,
            node,
            deposited: false,
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<usize, Round>> {
        self.rounds.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Deposits `deposit` for `node`; when it is the last, gives back every
    /// result of the round in node order, or ends the round not written when
    /// a node failed. `None` while other nodes have not deposited.
    fn deposit(&self, superstep: usize, node: usize, deposit: Deposit) -> Option<Vec<ToolResult>> {
        let all = {
            let mut rounds = self.lock();
            let round = rounds.get_mut(&superstep)?;
            if let Some(place) = round.deposits.get_mut(node) {
                *place = Some(deposit);
            }
            if round.deposits.iter().any(Option::is_none) {
                return None;
            }
            let mut all = Some(Vec::new());
            for deposit in round.deposits.iter_mut().map(Option::take) {
                match (deposit, &mut all) {
                    (Some(Deposit::Results(results)), Some(all)) => all.extend(results),
                    (Some(Deposit::Results(_)), None) => {}
                    (Some(Deposit::Failed) | None, _) => all = None,
                }
            }
            all
        };
        if all.is_none() {
            self.finish(superstep, Outcome::NotWritten);
        }
        all
    }

    /// Ends the round with `outcome` and wakes the tool nodes waiting for it.
    fn finish(&self, superstep: usize, outcome: Outcome) {
        let wakers = {
            let mut rounds = self.lock();
            match rounds.get_mut(&superstep) {
                Some(round) => {
                    round.outcome = Some(outcome);
                    std::mem::take(&mut round.wakers)
                }
                None => Vec::new(),
            }
        };
        for waker in wakers {
            waker.wake();
        }
    }
}

/// A tool node's place in the round of its superstep. Dropped before it
/// deposits (an error, a panic, a cancel), it deposits a failure, so the
/// other tool nodes never wait for it.
pub(crate) struct Seat {
    writers: Arc<RoundWriters>,
    superstep: usize,
    node: usize,
    deposited: bool,
}

impl Seat {
    /// Deposits this node's `results`; the last node of the round pushes
    /// every result of the round, in node order, through `write`. Then waits
    /// for the round to end: an error of the store fails every tool node with
    /// `Thread`; a round not written, because another node failed, lets this
    /// one give back its updates, which the failed superstep drops.
    pub(crate) async fn write<F, W>(
        mut self,
        results: Vec<ToolResult>,
        write: W,
    ) -> Result<(), GraphError>
    where
        W: FnOnce(Vec<ToolResult>) -> F,
        F: Future<Output = Result<(), GraphError>>,
    {
        self.deposited = true;
        let all = self
            .writers
            .deposit(self.superstep, self.node, Deposit::Results(results));
        if let Some(all) = all {
            let outcome = match write(all).await {
                Ok(()) => Outcome::Written,
                Err(GraphError::Thread { source }) => Outcome::StoreFailed(source),
                Err(other) => Outcome::StoreFailed(Arc::new(other)),
            };
            self.writers.finish(self.superstep, outcome);
        }
        match (RoundEnd { seat: &self }).await {
            Outcome::Written | Outcome::NotWritten => Ok(()),
            Outcome::StoreFailed(source) => Err(GraphError::Thread { source }),
        }
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        if !self.deposited {
            self.deposited = true;
            let _ = self
                .writers
                .deposit(self.superstep, self.node, Deposit::Failed);
        }
        let mut rounds = self.writers.lock();
        if let Some(round) = rounds.get_mut(&self.superstep) {
            round.holders = round.holders.saturating_sub(1);
            if round.holders == 0 {
                rounds.remove(&self.superstep);
            }
        }
    }
}

/// The wait for the end of a seat's round.
struct RoundEnd<'a> {
    seat: &'a Seat,
}

impl Future for RoundEnd<'_> {
    type Output = Outcome;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Outcome> {
        let mut rounds = self.seat.writers.lock();
        let Some(round) = rounds.get_mut(&self.seat.superstep) else {
            return Poll::Ready(Outcome::NotWritten);
        };
        match &round.outcome {
            Some(outcome) => Poll::Ready(outcome.clone()),
            None => {
                if !round.wakers.iter().any(|waker| waker.will_wake(cx.waker())) {
                    round.wakers.push(cx.waker().clone());
                }
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
#[path = "round_writer_tests.rs"]
mod tests;
