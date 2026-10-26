#![allow(clippy::panic)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use br_graph_contract::{CreateError, Fields, Instance, Reason};
use serde::{Deserialize, Serialize};

use super::Creation;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Unit(u32);

impl Fields for Unit {
    type Update = ();

    fn empty_update() {}

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, _update: ()) -> Result<(), Reason> {
        Ok(())
    }
}

struct Counting {
    polls: Arc<AtomicUsize>,
    panics: bool,
}

impl Future for Counting {
    type Output = Result<Instance<Unit>, CreateError>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if self.panics {
            std::panic::panic_any("inner panic");
        }
        Poll::Ready(Err(CreateError::new("refused")))
    }
}

fn running(panics: bool) -> (Creation<'static, Unit>, Arc<AtomicUsize>) {
    let polls = Arc::new(AtomicUsize::new(0));
    let future = Counting {
        polls: Arc::clone(&polls),
        panics,
    };
    (Creation::Running(Box::pin(future)), polls)
}

fn poll_once(
    creation: &mut Creation<'static, Unit>,
) -> Poll<<Creation<'static, Unit> as Future>::Output> {
    Pin::new(creation).poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn given_a_creation_that_was_ready_when_it_is_polled_again_then_it_is_pending_and_the_inner_future_is_not_polled()
 {
    let (mut creation, polls) = running(false);

    let first = poll_once(&mut creation);
    let second = poll_once(&mut creation);

    assert!(matches!(first, Poll::Ready(Ok(Err(_)))));
    assert!(second.is_pending());
    assert_eq!(polls.load(Ordering::SeqCst), 1);
}

#[test]
fn given_a_creation_that_panicked_in_its_poll_when_it_is_polled_again_then_it_is_pending_and_the_inner_future_is_not_polled()
 {
    let (mut creation, polls) = running(true);

    let first = poll_once(&mut creation);
    let second = poll_once(&mut creation);

    assert!(matches!(first, Poll::Ready(Err(text)) if text == "inner panic"));
    assert!(second.is_pending());
    assert_eq!(polls.load(Ordering::SeqCst), 1);
}

#[test]
fn given_a_creation_that_panicked_in_the_call_when_it_is_polled_twice_then_the_text_comes_once() {
    let mut creation = Creation::<Unit>::Panicked("call panic".to_owned());

    let first = poll_once(&mut creation);
    let second = poll_once(&mut creation);

    assert!(matches!(first, Poll::Ready(Err(text)) if text == "call panic"));
    assert!(second.is_pending());
}
