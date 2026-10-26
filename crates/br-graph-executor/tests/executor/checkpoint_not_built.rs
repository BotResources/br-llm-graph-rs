use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{CancelToken, State};
use br_graph_executor::{Outcome, RunConfig, start};
use tokio::sync::Notify;

use crate::flaky_fields::*;
use crate::host::within;

#[tokio::test(start_paused = true)]
async fn given_no_sink_when_the_state_cannot_be_serialized_in_the_step_that_ends_the_run_then_the_run_ends_with_the_final_state()
 {
    plan(&[Answer::Succeed, Answer::Succeed], Answer::Fail);
    let graph = graph_of(vec![ScriptedNode {
        name: "a",
        behavior: Behavior::Result,
    }]);
    let config = RunConfig::new(CancelToken::new(), Duration::from_secs(10));
    assert!(config.sink.is_none());
    let initial = State::create(Flaky(7)).unwrap();

    let outcome = within(start(graph, initial, vec!["a".to_owned()], config))
        .await
        .unwrap();

    match outcome {
        Outcome::RunEnded { state } => assert_eq!(state.into_fields(), Flaky(7)),
        other => panic!("expected RunEnded, got {other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn given_no_sink_when_a_result_is_applied_while_the_run_is_stopping_then_no_checkpoint_is_built_by_that_step()
 {
    plan(
        &[Answer::Succeed, Answer::Succeed, Answer::Succeed],
        Answer::Fail,
    );
    let release = Arc::new(Notify::new());
    let graph = graph_of(vec![
        ScriptedNode {
            name: "a",
            behavior: Behavior::ResultAfter(Arc::clone(&release)),
        },
        ScriptedNode {
            name: "b",
            behavior: Behavior::Silent,
        },
    ]);
    let token = CancelToken::new();
    let cancel = token.handle().clone();
    let config = RunConfig::new(token, Duration::from_secs(10));
    assert!(config.sink.is_none());
    let initial = State::create(Flaky(7)).unwrap();
    let run = tokio::spawn(start(
        graph,
        initial,
        vec!["a".to_owned(), "b".to_owned()],
        config,
    ));
    tokio::time::sleep(Duration::from_millis(1)).await;
    cancel.trigger();
    tokio::time::sleep(Duration::from_millis(1)).await;

    release.notify_one();
    let outcome = within(run).await.unwrap().unwrap();

    match outcome {
        Outcome::RunCancelled {
            failures,
            checkpoint,
        } => {
            assert!(failures.is_empty());
            assert_eq!(checkpoint.state_json(), "7");
            assert_eq!(checkpoint.nodes(), ["b"]);
        }
        other => panic!("expected RunCancelled, got {other:?}"),
    }
}
