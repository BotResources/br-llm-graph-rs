use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{CancelToken, Graph, State};
use br_graph_executor::{
    Checkpoint, Observation, Outcome, OutcomeKind, ResumeError, RunConfig, RunFailure, resume,
    start,
};
use tokio::sync::Notify;

use crate::flaky_fields::*;
use crate::host::{BIG, within};
use crate::observer::Recorder;

struct FailedRun {
    graph: Arc<Graph<Flaky>>,
    failures: Vec<RunFailure>,
    checkpoint: Checkpoint,
}

async fn failed_run(initial: u32, script: &[Answer], then: Answer) -> FailedRun {
    plan(script, then);
    let graph = graph_of(vec![ScriptedNode {
        name: "a",
        behavior: Behavior::Fail,
    }]);
    let initial = State::create(Flaky(initial)).unwrap();
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    let outcome = within(start(
        Arc::clone(&graph),
        initial,
        vec!["a".to_owned()],
        config,
    ))
    .await
    .unwrap();
    match outcome {
        Outcome::RunFailed {
            failures,
            checkpoint,
        } => FailedRun {
            graph,
            failures,
            checkpoint,
        },
        other => panic!("expected RunFailed, got {other:?}"),
    }
}

async fn run_with_exhausted_serialization(panics: bool) -> FailedRun {
    let then = if panics { Answer::Panic } else { Answer::Fail };
    failed_run(7, &[Answer::Succeed], then).await
}

async fn resume_refusal(run: FailedRun) -> ResumeError {
    plan(&[], Answer::Succeed);
    let config = RunConfig::new(CancelToken::new(), Duration::ZERO);
    match within(resume(run.graph, run.checkpoint, BIG, config)).await {
        Err(error) => error,
        Ok(outcome) => panic!("expected a refusal, got {outcome:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_unserializable_state_when_the_run_stops_then_the_checkpoint_state_is_empty_text()
{
    let run = run_with_exhausted_serialization(false).await;
    assert!(matches!(run.failures[0], RunFailure::NodeFailed { .. }));
    assert!(matches!(
        run.failures[1],
        RunFailure::CheckpointNotBuilt { .. }
    ));
    assert_eq!(run.checkpoint.state_json(), "");
}

#[tokio::test(start_paused = true)]
async fn given_an_unreadable_checkpoint_when_resumed_for_fields_that_accept_null_then_the_state_is_rejected()
 {
    let run = run_with_exhausted_serialization(false).await;
    plan(&[], Answer::Succeed);
    assert_eq!(
        State::<Flaky>::rebuild_with_limit(b"null", BIG)
            .unwrap()
            .into_fields(),
        Flaky(0)
    );
    let refused = resume_refusal(run).await;
    assert!(matches!(refused, ResumeError::StateRejected { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_state_panicking_in_deserialization_when_resumed_then_it_is_rejected_with_the_panic_text()
 {
    let run = failed_run(PANICS_WHEN_DESERIALIZED, &[], Answer::Succeed).await;
    assert_eq!(run.checkpoint.state_json(), "13");
    let refused = resume_refusal(run).await;
    assert_eq!(
        refused.to_string(),
        "checkpoint state cannot be rebuilt: deserialization panics"
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_state_that_panics_when_serialized_then_the_checkpoint_failure_is_reported_the_same_way()
 {
    let run = run_with_exhausted_serialization(true).await;
    match &run.failures[1] {
        RunFailure::CheckpointNotBuilt { reason } => {
            assert!(reason.contains("serialization panics"));
        }
        other => panic!("unexpected failure {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_an_unbuildable_sink_checkpoint_when_the_other_node_ends_in_the_grace_then_the_run_is_failed()
 {
    plan(
        &[Answer::Succeed, Answer::Succeed, Answer::Fail],
        Answer::Succeed,
    );
    let gate = Arc::new(Notify::new());
    let graph = graph_of(vec![
        ScriptedNode {
            name: "a",
            behavior: Behavior::Result,
        },
        ScriptedNode {
            name: "b",
            behavior: Behavior::ResultAfter(Arc::clone(&gate)),
        },
    ]);
    let observer = Recorder::new();
    let mut config = RunConfig::new(CancelToken::new(), Duration::from_secs(10));
    config.sink = Some(Arc::new(AcceptingSink));
    config.observer = Some(Arc::clone(&observer) as _);
    let initial = State::create(Flaky(7)).unwrap();
    gate.notify_one();
    let outcome = within(start(
        graph,
        initial,
        vec!["a".to_owned(), "b".to_owned()],
        config,
    ))
    .await
    .unwrap();
    let log = within(observer.wait_for_outcome()).await;
    match outcome {
        Outcome::RunFailed {
            failures,
            checkpoint,
        } => {
            assert!(matches!(failures[0], RunFailure::CheckpointNotBuilt { .. }));
            assert_eq!(failures.len(), 1);
            assert!(checkpoint.nodes().is_empty());
        }
        other => panic!("expected RunFailed, got {other:?}"),
    }
    assert_eq!(
        log.last(),
        Some(&Observation::Outcome(OutcomeKind::RunFailed))
    );
}

#[tokio::test(start_paused = true)]
async fn given_no_sink_when_the_state_cannot_be_serialized_in_a_result_step_then_the_run_fails_with_checkpoint_not_built()
 {
    plan(
        &[Answer::Succeed, Answer::Succeed, Answer::Fail],
        Answer::Succeed,
    );
    let gate = Arc::new(Notify::new());
    let graph = graph_of(vec![
        ScriptedNode {
            name: "a",
            behavior: Behavior::Result,
        },
        ScriptedNode {
            name: "b",
            behavior: Behavior::ResultAfter(Arc::clone(&gate)),
        },
    ]);
    let config = RunConfig::new(CancelToken::new(), Duration::from_secs(10));
    assert!(config.sink.is_none());
    let initial = State::create(Flaky(7)).unwrap();
    gate.notify_one();

    let outcome = within(start(
        graph,
        initial,
        vec!["a".to_owned(), "b".to_owned()],
        config,
    ))
    .await
    .unwrap();

    match outcome {
        Outcome::RunFailed {
            failures,
            checkpoint,
        } => {
            assert_eq!(failures.len(), 1);
            assert!(matches!(failures[0], RunFailure::CheckpointNotBuilt { .. }));
            assert!(checkpoint.nodes().is_empty());
        }
        other => panic!("expected RunFailed, got {other:?}"),
    }
}
