use std::mem;
use std::num::NonZeroUsize;
use std::sync::Arc;

use br_graph_contract::{CancelView, Event, EventEndError, Fields, State};
use tokio::task::JoinSet;

use super::scope::ChildScope;
use super::{MapReduceError, Rules, initial};
use crate::signals::Signals;

pub(super) enum Outcome<F: Fields> {
    Result(F::Update),
    Cancelled,
    Failed(MapReduceError),
    Dropped,
}

pub(super) enum Halt {
    Cancel,
    Dropped,
    Fault(MapReduceError),
}

pub(super) type ChildEnd<G> = (usize, Result<Event<G>, EventEndError>);

pub(super) struct Work<'a, F: Fields, G: Fields> {
    pub(super) rules: &'a Rules<F, G>,
    pub(super) scope: ChildScope,
    pub(super) states: Vec<Arc<State<G>>>,
    pub(super) children: JoinSet<ChildEnd<G>>,
    pub(super) next: usize,
}

pub(super) async fn run<F: Fields, G: Fields>(
    rules: &Rules<F, G>,
    internal_size_limit: NonZeroUsize,
    state: &State<F>,
    cancel: &CancelView,
    signals: &mut Signals,
) -> Outcome<F> {
    let states = match initial::states(rules, internal_size_limit, state) {
        Ok(states) => states,
        Err(error) => return Outcome::Failed(error),
    };
    let work = Work {
        rules,
        scope: ChildScope::new(cancel),
        states,
        children: JoinSet::new(),
        next: 0,
    };
    work.drive(signals).await
}

impl<F: Fields, G: Fields> Work<'_, F, G> {
    async fn drive(mut self, signals: &mut Signals) -> Outcome<F> {
        match self.complete(signals).await {
            Ok(outcome) => outcome,
            Err(Halt::Dropped) => Outcome::Dropped,
            Err(Halt::Cancel) => self.stop_cancelled(signals).await,
            Err(Halt::Fault(error)) => self.stop_failed(error).await,
        }
    }

    async fn complete(&mut self, signals: &mut Signals) -> Result<Outcome<F>, Halt> {
        loop {
            self.start_available(signals).await?;
            if self.children.is_empty() {
                return Ok(self.finish());
            }
            tokio::select! {
                biased;
                () = &mut signals.dropped => return Err(Halt::Dropped),
                () = &mut signals.cancel => return Err(Halt::Cancel),
                joined = self.children.join_next() => match joined {
                    Some(Ok((position, received))) => self.receive(position, received)?,
                    Some(Err(_)) | None => return Err(Halt::Dropped),
                },
            }
        }
    }

    async fn start_available(&mut self, signals: &mut Signals) -> Result<(), Halt> {
        while self.next < self.states.len() && self.children.len() < self.rules.limit.get() {
            if self.scope.is_triggered() {
                return Err(Halt::Cancel);
            }
            self.start(signals).await?;
        }
        Ok(())
    }

    fn finish(&mut self) -> Outcome<F> {
        if self.scope.is_triggered() {
            return Outcome::Cancelled;
        }
        let states: Vec<State<G>> = mem::take(&mut self.states)
            .into_iter()
            .map(|state| Arc::try_unwrap(state).unwrap_or_else(|shared| (*shared).clone()))
            .collect();
        Outcome::Result((self.rules.reduce)(&states))
    }

    async fn stop_failed(&mut self, error: MapReduceError) -> Outcome<F> {
        self.scope.trigger();
        self.children.shutdown().await;
        Outcome::Failed(error)
    }

    async fn stop_cancelled(&mut self, signals: &mut Signals) -> Outcome<F> {
        loop {
            tokio::select! {
                biased;
                () = &mut signals.dropped => return Outcome::Dropped,
                joined = self.children.join_next() => if joined.is_none() {
                    return Outcome::Cancelled;
                },
            }
        }
    }
}
