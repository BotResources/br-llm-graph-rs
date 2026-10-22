//! The test runner port's operations: each call kept with its key and
//! counted while it lives, held while its target or its key is, then
//! answered as its target's script says; a call dropped before it ends is
//! counted dropped.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use crate::runner::{CallKey, RunnerFuture, RunnerPort, RunnerRequest};
use crate::testkit::runners::TestRunners;

/// One call under way: counted while it lives, and counted dropped when it
/// is dropped before it ends.
struct InFlight<'a> {
    runners: &'a TestRunners,
    key: CallKey,
    ended: bool,
}

impl<'a> InFlight<'a> {
    fn start(runners: &'a TestRunners, request: &RunnerRequest) -> Self {
        let mut inner = runners.lock();
        inner.calls.push(request.clone());
        inner.live += 1;
        inner.peak = inner.peak.max(inner.live);
        Self {
            runners,
            key: request.key.clone(),
            ended: false,
        }
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        let mut inner = self.runners.lock();
        inner.live -= 1;
        if !self.ended {
            inner.dropped.push(self.key.clone());
        }
    }
}

/// The wait of a call while it is held, by its target or by its key.
struct Held<'a> {
    runners: &'a TestRunners,
    request: &'a RunnerRequest,
    counted: bool,
}

impl Future for Held<'_> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let runners = self.runners;
        let mut inner = runners.lock();
        if !inner.holds(self.request) {
            if self.counted {
                inner.waiting -= 1;
                self.counted = false;
            }
            return Poll::Ready(());
        }
        if !inner
            .held_wakers
            .iter()
            .any(|held| held.will_wake(cx.waker()))
        {
            inner.held_wakers.push(cx.waker().clone());
        }
        if self.counted {
            return Poll::Pending;
        }
        self.counted = true;
        inner.waiting += 1;
        let watchers = std::mem::take(&mut inner.watchers);
        drop(inner);
        watchers.into_iter().for_each(Waker::wake);
        Poll::Pending
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        if self.counted {
            self.runners.lock().waiting -= 1;
        }
    }
}

impl RunnerPort for TestRunners {
    fn call<'a>(&'a self, request: &'a RunnerRequest) -> RunnerFuture<'a> {
        Box::pin(async move {
            let mut flight = InFlight::start(self, request);
            Held {
                runners: self,
                request,
                counted: false,
            }
            .await;
            let reply = self.reply(request);
            flight.ended = true;
            reply
        })
    }
}
