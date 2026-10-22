//! A runner call as a node: one call through the port under the node's key,
//! its answer and its child's thread written as data, an expected failure of
//! the child as `RunnerFailed`, an error of the port as `Runner`, and a
//! cancel that drops the call under way.

use std::sync::Arc;

use serde_json::{Map, Value as Json, json};

use crate::error::{GraphError, NodeFault};
use crate::graph::TextOutput;
use crate::react::test_gate::beside;
use crate::run::{Outcome, control};
use crate::runner::test_support::*;
use crate::runner::{AnswerBlock, RunnerArguments, RunnerCall};
use crate::testkit::{PortError, TestRunners, text_block};

/// The error a failed run's node returned, read back as a `GraphError`.
fn returned(error: &GraphError) -> &GraphError {
    match error {
        GraphError::NodeFailed {
            node,
            source: NodeFault::Returned(returned),
        } if node.as_str() == "ask" => returned.downcast_ref::<GraphError>().unwrap(),
        other => panic!("expected the node ask to fail, got: {other}"),
    }
}

#[tokio::test]
async fn given_an_answer_when_the_node_runs_then_its_text_is_set_and_the_call_is_keyed_by_the_node_origin()
 {
    let port = Arc::new(TestRunners::new());
    port.answer(
        "documentalist",
        vec![text_block("Tides follow "), text_block("the moon.")],
    );
    let state = finished(
        run_in(
            &alone(ask(&port, set_answer())),
            state_with("tides?", &[]),
            &ctx("run-1"),
        )
        .await,
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "Tides follow the moon.");
    let calls = port.calls();
    let [call] = calls.as_slice() else {
        panic!("one call, got {calls:?}");
    };
    assert_eq!(call.key, call_key("run-1", "ask", None, &in_french()));
    assert_eq!(call.target.as_str(), "documentalist");
    assert_eq!(call.arguments, in_french());
}

#[tokio::test]
async fn given_an_append_output_and_a_thread_output_when_run_then_the_answer_is_appended_and_the_thread_set()
 {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("an answer")]);
    let node = RunnerCall {
        thread: Some(TextOutput::Set(skey("thread"))),
        ..ask(&port, append_answers())
    };
    let state = finished(run_in(&alone(node), state_with("", &[]), &ctx("run-1")).await);

    assert_eq!(texts(&state, "answers"), ["an answer"]);
    assert_eq!(
        state.str(&key("thread")).unwrap(),
        port.thread_of(&call_key("run-1", "ask", None, &in_french()))
            .as_str()
    );
}

#[tokio::test]
async fn given_a_structured_answer_when_run_then_the_json_text_of_its_structured_block_is_written()
{
    let port = Arc::new(TestRunners::new());
    port.answer(
        "documentalist",
        vec![
            text_block("Here is the selection."),
            AnswerBlock::Structured(json!({"document_ids": ["d1", "d2"]})),
            AnswerBlock::Structured(json!({"ignored": true})),
        ],
    );
    let state = finished(
        run_in(
            &alone(ask(&port, set_answer())),
            state_with("", &[]),
            &ctx("run-1"),
        )
        .await,
    );

    assert_eq!(
        state.str(&key("answer")).unwrap(),
        r#"{"document_ids":["d1","d2"]}"#
    );
}

#[tokio::test]
async fn given_arguments_given_by_a_function_when_run_then_the_port_gets_what_it_gives_for_the_state_and_configuration()
 {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("ok")]);
    let question = skey::<String>("question");
    let lang = crate::state::Schema::builder()
        .config::<String>("lang")
        .unwrap();
    let node = RunnerCall {
        arguments: RunnerArguments::from_fn(move |state, config| {
            Ok(Map::from_iter([
                ("question".to_owned(), Json::from(state.get(&question)?)),
                ("lang".to_owned(), Json::from(config.get(&lang)?)),
            ]))
        }),
        ..ask(&port, set_answer())
    };
    finished(run_in(&alone(node), state_with("tides?", &[]), &ctx("run-1")).await);

    assert_eq!(
        Json::Object(port.calls()[0].arguments.clone()),
        json!({"question": "tides?", "lang": "fr"})
    );
}

#[tokio::test]
async fn given_an_arguments_function_that_fails_when_run_then_the_node_fails_with_its_error_and_nothing_is_called()
 {
    let port = Arc::new(TestRunners::new());
    let node = RunnerCall {
        arguments: RunnerArguments::from_fn(|state, _| {
            state.get(&skey::<String>("missing"))?;
            Ok(Map::new())
        }),
        ..ask(&port, set_answer())
    };
    let failure = run_in(&alone(node), state_with("", &[]), &ctx("run-1"))
        .await
        .err()
        .unwrap();

    assert!(matches!(
        returned(&failure.error),
        GraphError::UnknownKey { key: found } if *found == key("missing")
    ));
    assert!(port.calls().is_empty());
}

#[tokio::test]
async fn given_a_child_that_fails_when_run_then_the_node_fails_with_runner_failed_naming_the_target_and_the_reason()
 {
    let port = Arc::new(TestRunners::new());
    port.fail("documentalist", "the child job failed");
    let failure = run_in(
        &alone(ask(&port, set_answer())),
        state_with("", &[]),
        &ctx("run-1"),
    )
    .await
    .err()
    .unwrap();

    let error = returned(&failure.error);
    assert!(matches!(
        error,
        GraphError::RunnerFailed { target: named, reason }
            if named.as_str() == "documentalist" && reason == "the child job failed"
    ));
    assert_eq!(error.to_string(), "the child job failed");
    assert!(std::error::Error::source(error).is_none());
    assert_eq!(failure.checkpoint.state.str(&key("answer")).unwrap(), "");
}

#[tokio::test]
async fn given_a_port_that_fails_when_run_then_the_node_fails_with_runner_carrying_its_error() {
    let port = Arc::new(TestRunners::new());
    port.break_down("documentalist", "the bus is down");
    let failure = run_in(
        &alone(ask(&port, set_answer())),
        state_with("", &[]),
        &ctx("run-1"),
    )
    .await
    .err()
    .unwrap();

    let error = returned(&failure.error);
    assert!(matches!(error, GraphError::Runner { .. }));
    assert_eq!(error.to_string(), "runner port error: the bus is down");
    let source = std::error::Error::source(error).unwrap();
    assert_eq!(
        source.downcast_ref::<PortError>(),
        Some(&PortError("the bus is down".to_owned()))
    );
}

#[tokio::test]
async fn given_a_cancel_while_the_call_waits_when_run_then_the_call_is_dropped_once_and_nothing_is_written()
 {
    let port = Arc::new(TestRunners::new());
    port.answer("documentalist", vec![text_block("too late")]);
    port.hold("documentalist");
    let graph = alone(ask(&port, set_answer()));
    let (sender, mut channel) = control();
    let ctx = ctx("run-1");
    let cancel = async {
        port.waiting(1).await;
        sender.cancel();
    };

    let (outcome, ()) = match beside(
        run_with(&graph, state_with("", &[]), &ctx, &mut channel),
        cancel,
    )
    .await
    {
        Ok(both) => both,
        Err(ended) => panic!("the run ended before the call waited: {:?}", ended.is_ok()),
    };

    let Ok(Outcome::Cancelled { checkpoint }) = outcome else {
        panic!("expected cancelled");
    };
    assert_eq!(checkpoint.state.str(&key("answer")).unwrap(), "");
    assert_eq!(
        port.dropped(),
        [call_key("run-1", "ask", None, &in_french())]
    );
    assert_eq!(port.calls().len(), 1);
}
