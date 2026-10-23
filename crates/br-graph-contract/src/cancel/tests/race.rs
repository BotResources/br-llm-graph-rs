use std::sync::{Arc, Barrier};

use super::super::{CancelToken, CancelView};

#[test]
fn given_a_race_of_trigger_and_child_when_both_finish_then_no_child_escapes_the_trigger() {
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
                            let parent_seen_triggered = view.is_triggered();
                            let child = view.child();
                            if parent_seen_triggered {
                                assert!(child.view().is_triggered());
                            }
                            child
                        })
                        .collect::<Vec<CancelToken>>()
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
        let children: Vec<CancelToken> = spawners
            .into_iter()
            .flat_map(|spawner| spawner.join().unwrap())
            .collect();
        trigger.join().unwrap();
        assert!(children.iter().all(|child| child.view().is_triggered()));
    }
}
