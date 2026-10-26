use crate::graphs::*;
use crate::host::*;
use crate::kv::*;

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
