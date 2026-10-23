use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Wake, Waker};

use super::super::{CancelToken, CancelView};
use super::support::{counting_waker, finishes_within, poll_with};

struct ObservesChild {
    child: CancelView,
    saw_child_triggered: AtomicBool,
}

impl Wake for ObservesChild {
    fn wake(self: Arc<Self>) {
        self.saw_child_triggered
            .store(self.child.is_triggered(), Ordering::SeqCst);
    }
}

struct PanicsOnWake;

impl Wake for PanicsOnWake {
    fn wake(self: Arc<Self>) {
        panic_any("the waker panicked");
    }
}

struct Reenters {
    view: CancelView,
}

impl Wake for Reenters {
    fn wake(self: Arc<Self>) {
        let _ = self;
    }
}

impl Drop for Reenters {
    fn drop(&mut self) {
        let child = self.view.child();
        drop(child);
        let _ = self.view.is_triggered();
    }
}

#[test]
fn given_a_waiting_parent_when_triggered_then_its_waker_sees_the_child_already_triggered() {
    let parent = CancelToken::new();
    let child = parent.child();
    let grandchild = child.child();
    let observer = Arc::new(ObservesChild {
        child: grandchild.view(),
        saw_child_triggered: AtomicBool::new(false),
    });
    let waker = Waker::from(Arc::clone(&observer));
    let view = parent.view();
    let mut future = pin!(view.triggered());
    assert!(poll_with(&mut future, &waker).is_pending());
    parent.handle().trigger();
    assert!(observer.saw_child_triggered.load(Ordering::SeqCst));
}

#[test]
fn given_a_waker_that_panics_when_triggered_then_every_other_waker_is_woken_and_the_panic_is_raised()
 {
    let parent = CancelToken::new();
    let child = parent.child();
    let (parent_count, parent_waker) = counting_waker();
    let (child_count, child_waker) = counting_waker();
    let (other_count, other_waker) = counting_waker();
    let panicking = Waker::from(Arc::new(PanicsOnWake));
    let parent_view = parent.view();
    let child_view = child.view();
    let mut first = pin!(parent_view.triggered());
    let mut second = pin!(parent_view.triggered());
    let mut third = pin!(parent_view.triggered());
    let mut fourth = pin!(child_view.triggered());
    assert!(poll_with(&mut first, &parent_waker).is_pending());
    assert!(poll_with(&mut second, &panicking).is_pending());
    assert!(poll_with(&mut third, &other_waker).is_pending());
    assert!(poll_with(&mut fourth, &child_waker).is_pending());

    let outcome = catch_unwind(AssertUnwindSafe(|| parent.handle().trigger()));

    let payload = outcome.unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"the waker panicked"));
    assert_eq!(parent_count.value(), 1);
    assert_eq!(other_count.value(), 1);
    assert_eq!(child_count.value(), 1);
    assert!(child.view().is_triggered());
    assert!(
        first
            .as_mut()
            .poll(&mut std::task::Context::from_waker(&parent_waker))
            .is_ready()
    );
}

#[test]
fn given_several_panicking_wakers_when_triggered_then_the_trigger_still_wakes_all_and_raises_once()
{
    let token = CancelToken::new();
    let view = token.view();
    let woken = Arc::new(AtomicUsize::new(0));
    struct Counts(Arc<AtomicUsize>);
    impl Wake for Counts {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let panicking = Waker::from(Arc::new(PanicsOnWake));
    let counting = Waker::from(Arc::new(Counts(Arc::clone(&woken))));
    let mut futures: Vec<_> = (0..6).map(|_| Box::pin(view.triggered())).collect();
    for (index, future) in futures.iter_mut().enumerate() {
        let waker = if index % 2 == 0 {
            &panicking
        } else {
            &counting
        };
        assert!(poll_with(&mut future.as_mut(), waker).is_pending());
    }
    assert!(catch_unwind(AssertUnwindSafe(|| token.handle().trigger())).is_err());
    assert_eq!(woken.load(Ordering::SeqCst), 3);
}

#[test]
fn given_a_waker_whose_drop_reenters_when_it_is_replaced_by_a_repoll_then_there_is_no_deadlock() {
    finishes_within(|| {
        let token = CancelToken::new();
        let view = token.view();
        let mut future = pin!(view.triggered());
        let reentrant = Waker::from(Arc::new(Reenters { view: view.clone() }));
        assert!(poll_with(&mut future, &reentrant).is_pending());
        drop(reentrant);
        let (_count, other) = counting_waker();
        assert!(poll_with(&mut future, &other).is_pending());
        assert_eq!(view.shared.waiter_count(), 1);
    });
}

#[test]
fn given_a_waker_whose_drop_reenters_when_its_future_is_dropped_then_there_is_no_deadlock() {
    finishes_within(|| {
        let token = CancelToken::new();
        let view = token.view();
        let reentrant = Waker::from(Arc::new(Reenters { view: view.clone() }));
        {
            let mut future = pin!(view.triggered());
            assert!(poll_with(&mut future, &reentrant).is_pending());
            drop(reentrant);
        }
        assert_eq!(view.shared.waiter_count(), 0);
    });
}

#[test]
fn given_a_waker_whose_drop_reenters_when_the_token_is_triggered_then_there_is_no_deadlock() {
    finishes_within(|| {
        let token = CancelToken::new();
        let view = token.view();
        let reentrant = Waker::from(Arc::new(Reenters { view: view.clone() }));
        let mut future = pin!(view.triggered());
        assert!(poll_with(&mut future, &reentrant).is_pending());
        drop(reentrant);
        token.handle().trigger();
        assert!(view.is_triggered());
    });
}
