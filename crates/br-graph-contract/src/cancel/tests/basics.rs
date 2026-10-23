use std::pin::pin;

use super::super::{CancelToken, Triggered};
use super::support::{counting_waker, poll_once, poll_with};

fn assert_owned_send<T: Send + 'static>() {}

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
fn given_the_parts_of_a_token_when_the_handle_triggers_then_the_view_is_triggered() {
    let (handle, view) = CancelToken::new().into_parts();
    assert!(!view.is_triggered());
    handle.trigger();
    assert!(view.is_triggered());
    assert!(handle.view().is_triggered());
}

#[test]
fn given_the_future_of_a_view_when_checked_then_it_is_owned_and_sendable() {
    assert_owned_send::<Triggered>();
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
    let (count, waker) = counting_waker();
    let token = CancelToken::new();
    let mut future = pin!(token.view().triggered());
    assert!(poll_with(&mut future, &waker).is_pending());
    token.handle().trigger();
    token.handle().trigger();
    assert_eq!(count.value(), 1);
    assert!(poll_with(&mut future, &waker).is_ready());
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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn given_many_waiting_tasks_when_triggered_from_another_thread_then_all_wake() {
    use std::time::Duration;

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
