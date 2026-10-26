use std::sync::Arc;

use br_graph_contract::CancelToken;
use br_graph_executor::{Observation, OutcomeKind, ResumeError, RunConfig, RunFailure, resume};
use tokio::sync::Notify;

use crate::graphs::*;
use crate::host::*;
use crate::kv::*;
use crate::observer::*;
use crate::sink::SinkBehavior;

#[tokio::test(start_paused = true)]
async fn given_a_refusing_sink_when_the_run_fails_then_the_failure_carries_the_refused_checkpoint()
{
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    sink.script(SinkBehavior::Refuse("disk full"));
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    let (failures, outcome_checkpoint) = failed(host.outcome().await);
    match &failures[0] {
        RunFailure::SinkRefused { reason, checkpoint } => {
            assert!(reason.contains("disk full"));
            assert_eq!(checkpoint, &sink.received()[0]);
            assert_eq!(checkpoint.nodes(), ["b"]);
            assert_eq!(state_of(checkpoint), kv(&[("k", 1)]));
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(outcome_checkpoint, sink.received()[0]);
}

#[tokio::test(start_paused = true)]
async fn given_a_refusing_sink_when_the_other_node_ends_in_the_grace_then_the_run_is_failed_not_ended()
 {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    sink.script(SinkBehavior::Refuse("disk full"));
    let observer = host.observer.clone();
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.result(set("a", 1));
    observer.wait_for(handled("a")).await;
    b.result(set("b", 1));
    let outcome = host.outcome().await;
    let log = within(observer.wait_for_outcome()).await;
    let (failures, checkpoint) = failed(outcome);
    assert!(matches!(&failures[0], RunFailure::SinkRefused { .. }));
    assert_eq!(failures.len(), 1);
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunFailed))
    );
    assert_eq!(state_of(&checkpoint), kv(&[("a", 1), ("b", 1)]));
    assert!(checkpoint.nodes().is_empty());
    let config = RunConfig::new(CancelToken::new(), GRACE);
    let resumed = within(resume(Arc::clone(&rig.graph), checkpoint, BIG, config)).await;
    assert!(matches!(resumed, Err(ResumeError::NoNodes)));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_in_the_sink_call_when_the_sink_refuses_then_the_run_is_cancelled_with_the_failure()
 {
    let rig = chain("1", &["a", "b"]);
    let host = begin(&rig, state(&[]), &["a"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let gate = Arc::new(Notify::new());
    sink.script(SinkBehavior::BlockThenRefuse(gate.clone(), "disk full"));
    let mut a = rig.stage.instance("a").await;
    a.result(set("k", 1));
    sink.wait_for_calls(1).await;
    host.cancel.trigger();
    gate.notify_one();
    let observer = host.observer.clone();
    let (failures, checkpoint) = cancelled(host.outcome().await);
    assert_eq!(failures.len(), 1);
    match &failures[0] {
        RunFailure::SinkRefused {
            reason,
            checkpoint: refused,
        } => {
            assert!(reason.contains("disk full"));
            assert_eq!(refused, &sink.received()[0]);
        }
        other => panic!("unexpected failure {other:?}"),
    }
    assert_eq!(checkpoint.nodes(), ["b"]);
    assert_eq!(rig.stage.create_calls("b"), 0);
    let log = within(observer.wait_for_outcome()).await;
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunCancelled))
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_host_cancel_in_the_sink_call_when_the_sink_refuses_and_the_other_node_ends_in_the_grace_then_the_run_is_cancelled_with_the_failure()
 {
    let rig = two_independent();
    let host = begin(&rig, state(&[]), &["a", "b"], Setup::new().sink());
    let sink = host.sink.clone().unwrap();
    let gate = Arc::new(Notify::new());
    sink.script(SinkBehavior::BlockThenRefuse(gate.clone(), "disk full"));
    let observer = host.observer.clone();
    let mut a = rig.stage.instance("a").await;
    let mut b = rig.stage.instance("b").await;
    a.result(set("a", 1));
    sink.wait_for_calls(1).await;
    host.cancel.trigger();
    gate.notify_one();
    b.result(set("b", 1));
    let outcome = host.outcome().await;
    let log = within(observer.wait_for_outcome()).await;
    let (failures, checkpoint) = cancelled(outcome);
    assert_eq!(failures.len(), 1);
    assert!(
        matches!(&failures[0], RunFailure::SinkRefused { reason, .. } if reason == "disk full")
    );
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunCancelled))
    );
    assert_eq!(state_of(&checkpoint), kv(&[("a", 1), ("b", 1)]));
    assert!(checkpoint.nodes().is_empty());
}
