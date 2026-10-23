use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};

use super::super::{CancelToken, CancelView};
use super::support::finishes_within;

fn chain_below(top: &CancelToken, depth: usize) -> CancelToken {
    let mut leaf = top.child();
    for _ in 1..depth {
        leaf = leaf.child();
    }
    leaf
}

#[test]
fn given_a_deep_chain_when_the_root_reads_triggered_then_the_leaf_reads_triggered() {
    finishes_within(|| {
        for _ in 0..20 {
            let root = CancelToken::new();
            let leaf = chain_below(&root, 50_000);
            let root_view = root.view();
            let leaf_view = leaf.view();
            let handle = root.handle().clone();
            let start = Arc::new(Barrier::new(2));
            let observer = {
                let start = Arc::clone(&start);
                std::thread::spawn(move || {
                    start.wait();
                    while !root_view.is_triggered() {
                        std::hint::spin_loop();
                    }
                    leaf_view.is_triggered()
                })
            };
            start.wait();
            handle.trigger();
            assert!(observer.join().unwrap());
        }
    });
}

#[test]
fn given_a_racing_trigger_of_a_middle_token_when_the_root_trigger_returns_then_the_leaf_is_triggered()
 {
    finishes_within(|| {
        for _ in 0..50 {
            let root = CancelToken::new();
            let middle = root.child();
            let leaf = chain_below(&middle, 20_000);
            let leaf_view: CancelView = leaf.view();
            let root_handle = root.handle().clone();
            let middle_handle = middle.handle().clone();
            let start = Arc::new(Barrier::new(3));
            let middle_done = Arc::new(AtomicBool::new(false));
            let middle_thread = {
                let start = Arc::clone(&start);
                let middle_done = Arc::clone(&middle_done);
                std::thread::spawn(move || {
                    start.wait();
                    middle_handle.trigger();
                    middle_done.store(true, Ordering::SeqCst);
                })
            };
            let root_thread = {
                let start = Arc::clone(&start);
                let leaf_view = leaf_view.clone();
                std::thread::spawn(move || {
                    start.wait();
                    root_handle.trigger();
                    leaf_view.is_triggered()
                })
            };
            start.wait();
            assert!(root_thread.join().unwrap());
            middle_thread.join().unwrap();
            assert!(leaf_view.is_triggered());
        }
    });
}
