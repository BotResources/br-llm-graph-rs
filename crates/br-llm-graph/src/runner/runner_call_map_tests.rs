//! A runner call as the body of a map: one call per item under the item's
//! own key, at most the map's concurrency limit at once, the answers
//! appended in item order; a child's failure captured as data; a `Set`
//! refused there.

use std::sync::Arc;

use crate::error::GraphError;
use crate::graph::{CaptureUpdate, CheckSite, ItemFailure, Node};
use crate::react::test_gate::beside;
use crate::runner::test_support::*;
use crate::runner::{RunnerCall, RunnerOutput};
use crate::testkit::{Scripted, TestRunners, text_block};

/// A port whose `documentalist` answers with the analysis of the document
/// it is asked about, and fails for `missing`.
fn analysing() -> Arc<TestRunners> {
    let port = Arc::new(TestRunners::new());
    port.script("documentalist", |request| {
        match request
            .arguments
            .get("document")
            .and_then(|value| value.as_str())
        {
            Some("missing") => Scripted::Fail("no such document".to_owned()),
            Some(document) => {
                Scripted::Answer(vec![text_block(&format!("analysis of {document}"))])
            }
            None => Scripted::Break("no document asked".to_owned()),
        }
    });
    port
}

#[tokio::test]
async fn given_a_runner_call_as_a_map_body_when_run_then_one_call_per_item_under_its_key_at_most_the_limit_at_once()
 {
    let port = analysing();
    port.hold("documentalist");
    let graph = mapped(ask_about_document(&port), Some(2), ItemFailure::Finish);
    let check = async {
        port.waiting(2).await;
        let started = port.calls().len();
        port.release("documentalist");
        started
    };
    let ctx = ctx("run-1");

    let (result, started) = match beside(
        run_in(&graph, state_with("", &["a", "b", "c", "d"]), &ctx),
        check,
    )
    .await
    {
        Ok(both) => both,
        Err(ended) => panic!("the run ended before two calls waited: {:?}", ended.is_ok()),
    };

    assert_eq!(started, 2, "no third call starts while two wait");
    let state = finished(result);
    assert_eq!(
        texts(&state, "answers"),
        ["a", "b", "c", "d"].map(|document| format!("analysis of {document}"))
    );
    assert_eq!(port.peak(), 2);
    let mut keys = port.keys();
    keys.sort_by_key(|key| key.origin.occurrence.to_string());
    assert_eq!(
        keys,
        [
            ("each[0]", "a"),
            ("each[1]", "b"),
            ("each[2]", "c"),
            ("each[3]", "d")
        ]
        .map(|(item, document)| call_key("run-1", item, None, &about(document)))
    );
}

#[tokio::test]
async fn given_a_child_failing_for_one_item_and_a_capture_when_run_then_its_reason_is_data_and_the_others_answer()
 {
    let port = analysing();
    let threads = RunnerCall {
        thread: Some(RunnerOutput::Append(skey("threads"))),
        ..ask_about_document(&port)
    };
    let graph = mapped(
        threads,
        None,
        ItemFailure::Capture(vec![CaptureUpdate::append_reason(&skey("failures"))]),
    );
    let state = finished(
        run_in(
            &graph,
            state_with("", &["a", "missing", "c"]),
            &ctx("run-1"),
        )
        .await,
    );

    assert_eq!(texts(&state, "answers"), ["analysis of a", "analysis of c"]);
    assert_eq!(texts(&state, "failures"), ["no such document"]);
    assert_eq!(
        texts(&state, "threads"),
        [("each[0]", "a"), ("each[2]", "c")].map(|(item, document)| {
            port.thread_of(&call_key("run-1", item, None, &about(document)))
                .as_str()
                .to_owned()
        })
    );
}

#[test]
fn given_a_set_output_or_a_set_thread_as_a_map_body_when_built_then_map_body_set() {
    let port = Arc::new(TestRunners::new());
    let set_thread = RunnerCall {
        thread: Some(RunnerOutput::Set(skey("thread"))),
        ..ask(&port, append_answers())
    };
    for (body, set) in [(ask(&port, set_answer()), "answer"), (set_thread, "thread")] {
        assert!(body.check(&schema(), CheckSite::Graph).is_ok(), "{set}");
        assert!(
            matches!(
                body.check(&schema(), CheckSite::MapBody),
                Err(GraphError::MapBodySet { key: found }) if found == key(set)
            ),
            "{set}"
        );
    }
}
