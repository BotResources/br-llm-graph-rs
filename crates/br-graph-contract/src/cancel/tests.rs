#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use super::{CancelToken, CancelView};

fn poll_once<T>(future: &mut std::pin::Pin<&mut impl std::future::Future<Output = T>>) -> Poll<T> {
    future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn given_a_new_token_when_read_then_it_is_not_triggered() {
    let token = CancelToken::new();
    assert!(!token.view().is_triggered());
    assert!(!token.handle().is_triggered());
}

#[test]
fn given_a_triggered_handle_when_a_view_clone_reads_then_it_is_triggered_for_ever() {
    let token = CancelToken::new();
    let view = token.view();
    let clone = view.clone();
    token.handle().trigger();
    token.handle().trigger();
    assert!(view.is_triggered());
    assert!(clone.is_triggered());
}

#[test]
fn given_a_parent_when_triggered_then_children_and_grandchildren_are_triggered_at_once() {
    let parent = CancelToken::new();
    let child = parent.view().child();
    let grandchild = child.view().child();
    parent.handle().trigger();
    assert!(child.view().is_triggered());
    assert!(grandchild.view().is_triggered());
}

#[test]
fn given_a_child_when_triggered_then_the_parent_and_siblings_are_not() {
    let parent = CancelToken::new();
    let child = parent.child();
    let sibling = parent.child();
    child.handle().trigger();
    assert!(!parent.view().is_triggered());
    assert!(!sibling.view().is_triggered());
}

#[test]
fn given_a_triggered_parent_when_a_child_is_made_then_it_is_born_triggered() {
    let parent = CancelToken::new();
    parent.handle().trigger();
    assert!(parent.view().child().view().is_triggered());
}

#[test]
fn given_a_very_deep_chain_when_the_root_is_triggered_then_every_level_is_triggered() {
    let root = CancelToken::new();
    let mut tokens = vec![root.child()];
    for _ in 0..20_000 {
        let next = tokens.last().unwrap().child();
        tokens.push(next);
    }
    root.handle().trigger();
    assert!(tokens.iter().all(|token| token.view().is_triggered()));
}

#[test]
fn given_a_view_when_polled_after_the_trigger_then_it_is_ready() {
    let token = CancelToken::new();
    let view = token.view();
    token.handle().trigger();
    let mut future = pin!(view.triggered());
    assert!(poll_once(&mut future).is_ready());
}

#[test]
fn given_a_polled_waiter_when_triggered_then_its_waker_is_woken_once() {
    struct Count(AtomicUsize);
    impl std::task::Wake for Count {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let count = Arc::new(Count(AtomicUsize::new(0)));
    let waker = Waker::from(Arc::clone(&count));
    let token = CancelToken::new();
    let mut future = pin!(token.view().triggered());
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    token.handle().trigger();
    token.handle().trigger();
    assert_eq!(count.0.load(Ordering::SeqCst), 1);
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_ready()
    );
}

#[test]
fn given_one_future_polled_many_times_when_inspected_then_it_holds_one_slot() {
    let token = CancelToken::new();
    let view = token.view();
    let mut future = pin!(view.triggered());
    for _ in 0..1_000 {
        assert!(poll_once(&mut future).is_pending());
    }
    assert_eq!(view.shared.waiter_count(), 1);
}

#[test]
fn given_a_dropped_waiting_future_when_inspected_then_its_slot_is_gone() {
    let token = CancelToken::new();
    let view = token.view();
    {
        let mut future = pin!(view.triggered());
        assert!(poll_once(&mut future).is_pending());
        assert_eq!(view.shared.waiter_count(), 1);
    }
    assert_eq!(view.shared.waiter_count(), 0);
}

#[tokio::test]
async fn given_a_select_loop_when_it_races_the_view_100k_times_then_no_slot_is_left() {
    let token = CancelToken::new();
    let view = token.view();
    for _ in 0..100_000 {
        tokio::select! {
            () = view.triggered() => unreachable!(),
            () = tokio::task::yield_now() => {}
        }
    }
    assert_eq!(view.shared.waiter_count(), 0);
}

#[test]
fn given_children_made_and_dropped_for_ever_when_inspected_then_the_parent_list_stays_small() {
    let token = CancelToken::new();
    let view = token.view();
    for _ in 0..100_000 {
        drop(view.child());
    }
    assert!(view.shared.child_slots() <= 1);
}

#[test]
fn given_live_children_when_inspected_then_none_is_pruned() {
    let token = CancelToken::new();
    let children: Vec<CancelToken> = (0..100).map(|_| token.child()).collect();
    assert_eq!(token.view().shared.child_slots(), 100);
    token.handle().trigger();
    assert!(children.iter().all(|child| child.view().is_triggered()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn given_many_waiting_tasks_when_triggered_from_another_thread_then_all_wake() {
    let token = CancelToken::new();
    let waiters: Vec<_> = (0..200)
        .map(|_| {
            let view = token.view();
            tokio::spawn(async move { view.triggered().await })
        })
        .collect();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let handle = token.handle().clone();
    std::thread::spawn(move || handle.trigger()).join().unwrap();
    for waiter in waiters {
        tokio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .unwrap()
            .unwrap();
    }
    assert_eq!(token.view().shared.waiter_count(), 0);
}

#[test]
fn given_a_race_of_trigger_and_child_when_both_finish_then_every_child_is_triggered() {
    let mut born_before = 0;
    let mut born_after = 0;
    for _ in 0..300 {
        let root = CancelToken::new();
        let view = root.view();
        let handle = root.handle().clone();
        let start = Arc::new(Barrier::new(6));
        let spawners: Vec<_> = (0..4)
            .map(|_| {
                let view: CancelView = view.clone();
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    (0..200)
                        .map(|_| {
                            let child = view.child();
                            let already = child.view().is_triggered();
                            (child, already)
                        })
                        .collect::<Vec<(CancelToken, bool)>>()
                })
            })
            .collect();
        let trigger = {
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                handle.trigger();
            })
        };
        start.wait();
        let children: Vec<(CancelToken, bool)> = spawners
            .into_iter()
            .flat_map(|spawner| spawner.join().unwrap())
            .collect();
        trigger.join().unwrap();
        assert!(
            children
                .iter()
                .all(|(child, _)| child.view().is_triggered())
        );
        born_after += children.iter().filter(|(_, already)| *already).count();
        born_before += children.iter().filter(|(_, already)| !*already).count();
    }
    assert!(born_before > 0 && born_after > 0);
}
