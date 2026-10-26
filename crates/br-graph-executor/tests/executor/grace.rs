use std::time::Duration;

use br_graph_contract::CancelToken;
use br_graph_executor::{RunConfig, start};

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::nodes::{Creation, DropFlag};

fn largest_grace(now: tokio::time::Instant) -> Duration {
    let (mut low, mut high) = (0u64, u64::MAX);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if now.checked_add(Duration::from_secs(middle)).is_some() {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let (mut low_nanos, mut high_nanos) = (0u32, 999_999_999u32);
    while low_nanos < high_nanos {
        let middle = low_nanos + (high_nanos - low_nanos).div_ceil(2);
        if now.checked_add(Duration::new(low, middle)).is_some() {
            low_nanos = middle;
        } else {
            high_nanos = middle - 1;
        }
    }
    Duration::new(low, low_nanos)
}

#[tokio::test(start_paused = true)]
async fn given_results_queued_when_the_grace_ends_then_both_are_applied_and_the_run_ended() {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new());
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    host.cancel.trigger();
    quiesce().await;
    a.result(set("a", 1));
    b.result(set("b", 1));
    tokio::time::advance(GRACE).await;
    assert_eq!(ended(host.outcome().await), kv(&[("a", 1), ("b", 1)]));
}

#[tokio::test(start_paused = true)]
async fn given_a_zero_grace_when_a_result_is_queued_with_the_cancel_then_the_instance_is_dropped_at_once()
 {
    let rig = single("a");
    let host = begin(
        &rig,
        state(&[]),
        &["a"],
        Setup::new().grace(std::time::Duration::ZERO),
    );
    let mut a = rig.stage.instance("a").await;
    a.result(set("a", 1));
    host.cancel.trigger();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert!(failures.is_empty());
    assert_eq!(checkpoint.nodes(), ["a"]);
    assert_eq!(state_of(&checkpoint), kv(&[]));
}

#[tokio::test(start_paused = true)]
async fn given_a_grace_just_below_the_instant_limit_when_a_node_fails_then_the_run_fails_without_panic()
 {
    let rig = two_independent();
    rig.stage.script("a", Creation::Fail("a cannot start"));
    rig.stage.script("b", Creation::Never(DropFlag::default()));
    let grace = largest_grace(tokio::time::Instant::now()) - Duration::from_nanos(1);
    let config = RunConfig::new(CancelToken::new(), grace);
    let outcome = start(
        rig.graph.clone(),
        state(&[]),
        vec!["a".to_owned(), "b".to_owned()],
        config,
    )
    .await
    .unwrap();
    let (failures, checkpoint) = failed(outcome);
    assert_eq!(failures.len(), 1);
    assert_eq!(checkpoint.nodes(), ["a", "b"]);
}
