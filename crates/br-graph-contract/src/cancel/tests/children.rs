use std::time::{Duration, Instant};

use super::super::CancelToken;

#[test]
fn given_children_made_and_dropped_for_ever_when_inspected_then_the_parent_list_stays_small() {
    let token = CancelToken::new();
    let view = token.view();
    for _ in 0..100_000 {
        drop(view.child());
    }
    assert!(view.shared.child_slots() <= 17);
    assert_eq!(view.shared.live_child_count(), 0);
}

#[test]
fn given_live_children_when_inspected_then_none_is_pruned() {
    let token = CancelToken::new();
    let children: Vec<CancelToken> = (0..100).map(|_| token.child()).collect();
    assert_eq!(token.view().shared.child_slots(), 100);
    assert_eq!(token.view().shared.live_child_count(), 100);
    token.handle().trigger();
    assert!(children.iter().all(|child| child.view().is_triggered()));
}

#[test]
fn given_steady_live_children_and_churn_when_inspected_then_the_list_is_bounded_by_the_live_count()
{
    let token = CancelToken::new();
    let view = token.view();
    let live: Vec<CancelToken> = (0..1_000).map(|_| view.child()).collect();
    for _ in 0..100_000 {
        drop(view.child());
        assert!(view.shared.child_slots() <= 2 * 1_001 + 17);
    }
    assert_eq!(view.shared.live_child_count(), 1_000);
    drop(live);
    assert_eq!(view.shared.live_child_count(), 0);
}

#[test]
fn given_a_long_lived_parent_with_one_child_per_item_when_items_end_then_the_list_does_not_grow() {
    let token = CancelToken::new();
    let view = token.view();
    let mut high_water = 0;
    for _ in 0..50_000 {
        let batch: Vec<CancelToken> = (0..10).map(|_| view.child()).collect();
        high_water = high_water.max(view.shared.child_slots());
        drop(batch);
    }
    assert!(high_water <= 2 * 10 + 17);
}

#[test]
fn given_80k_live_children_when_made_then_it_takes_linear_time() {
    let token = CancelToken::new();
    let view = token.view();
    let started = Instant::now();
    let children: Vec<CancelToken> = (0..80_000).map(|_| view.child()).collect();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(view.shared.live_child_count(), 80_000);
    token.handle().trigger();
    assert!(children.iter().all(|child| child.view().is_triggered()));
}

#[test]
fn given_a_triggered_parent_when_children_are_made_then_none_is_counted() {
    let token = CancelToken::new();
    token.handle().trigger();
    let child = token.child();
    assert_eq!(token.view().shared.live_child_count(), 0);
    assert_eq!(token.view().shared.child_slots(), 0);
    assert!(child.view().is_triggered());
}
