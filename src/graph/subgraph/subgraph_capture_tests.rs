use std::sync::{Arc, Mutex};

use super::test_support::*;
use crate::error::GraphError;
use crate::graph::subgraph::{CaptureSource, CaptureUpdate, Input, OnFailure, Output, SubGraph};
use crate::graph::{Always, Graph, GraphBuilder, Map};
use crate::observe::{NoopObserver, Observer};
use crate::origin::Origin;
use crate::run::Outcome;
use crate::state::{State, Value};
use crate::update::Update;

const REASON_BAD: &str = "node work failed: returned an error: cannot handle bad";

fn picky_call() -> SubGraph {
    SubGraph::call(picky_graph())
        .input(key("text"), Input::From(key("question")))
        .output(key("answer"), Output::Set(key("result")))
}

fn capture_into_result_and_label() -> OnFailure {
    OnFailure::Capture(vec![
        CaptureUpdate::Set(
            key("result"),
            CaptureSource::Const(Value::str("unavailable")),
        ),
        CaptureUpdate::Set(key("label"), CaptureSource::Reason),
    ])
}

#[tokio::test]
async fn given_a_capturing_call_whose_child_fails_when_run_then_the_run_finishes_with_the_captured_updates()
 {
    let graph = parent_with(picky_call().on_failure(capture_into_result_and_label())).unwrap();
    let state = finished(&graph, "bad", 1).await;
    assert_eq!(state.str(&key("result")).unwrap(), "unavailable");
    assert_eq!(state.str(&key("label")).unwrap(), REASON_BAD);
}

#[tokio::test]
async fn given_a_capturing_call_whose_child_succeeds_when_run_then_only_the_outputs_are_written() {
    let graph = parent_with(picky_call().on_failure(capture_into_result_and_label())).unwrap();
    let state = finished(&graph, "good", 1).await;
    assert_eq!(state.str(&key("result")).unwrap(), "good!");
    assert_eq!(state.str(&key("label")).unwrap(), "");
}

#[tokio::test]
async fn given_a_call_without_a_failure_mode_when_its_child_fails_then_the_run_fails() {
    let graph = parent_with(picky_call()).unwrap();
    let state = parent_state(&graph, "bad");
    let failure = run_parent(&graph, state, &parent_config(1), Arc::new(NoopObserver))
        .await
        .err()
        .unwrap();
    assert!(matches!(failure.error, GraphError::NodeFailed { .. }));
}

fn per_item(on_failure: OnFailure) -> SubGraph {
    SubGraph::call(picky_graph())
        .input(key("text"), Input::From(key("item")))
        .output(key("answer"), Output::Append(key("results")))
        .on_failure(on_failure)
}

fn capture_item_and_reason() -> OnFailure {
    OnFailure::Capture(vec![
        CaptureUpdate::Append(key("labels"), CaptureSource::From(key("item"))),
        CaptureUpdate::Append(key("reasons"), CaptureSource::Reason),
    ])
}

fn mapped(body: SubGraph) -> Result<Graph, GraphError> {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: None,
    };
    GraphBuilder::new(parent_schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(nid("each"), Always(end("done")))
        .input(key("items"))
        .build()
}

fn start(graph: &Graph, items: &[&str]) -> State {
    let items = Value::list(items.iter().map(|item| Value::str(*item)).collect());
    graph.start_state([(key("items"), items)]).unwrap()
}

#[derive(Default)]
struct Records(Mutex<Vec<(String, Vec<Update>)>>);

impl Observer for Records {
    fn recorded(&self, origin: &Origin, updates: &[Update]) {
        let entry = (origin.occurrence.to_string(), updates.to_vec());
        self.0.lock().unwrap().push(entry);
    }
}

#[tokio::test]
async fn given_failing_items_with_capture_when_mapped_then_failures_are_listed_in_item_order_and_recorded()
 {
    let graph = mapped(per_item(capture_item_and_reason())).unwrap();
    let records = Arc::new(Records::default());
    let items = ["ok1", "bad1", "ok2", "bad2", "ok3"];
    let outcome = run_parent(
        &graph,
        start(&graph, &items),
        &parent_config(1),
        records.clone(),
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("a captured failure must not fail the run");
    };
    assert_eq!(
        state.list(&key("results")).unwrap(),
        &["ok1!", "ok2!", "ok3!"].map(Value::str)
    );
    assert_eq!(
        state.list(&key("labels")).unwrap(),
        &["bad1", "bad2"].map(Value::str)
    );
    assert_eq!(
        state.list(&key("reasons")).unwrap(),
        &[
            "node work failed: returned an error: cannot handle bad1",
            "node work failed: returned an error: cannot handle bad2"
        ]
        .map(Value::str)
    );
    let records = records.0.lock().unwrap().clone();
    let failed = records
        .iter()
        .find(|(occurrence, _)| occurrence == "each[1]")
        .unwrap();
    assert_eq!(
        failed.1.first(),
        Some(&Update::Append {
            key: key("labels"),
            value: Value::str("bad1")
        })
    );
    assert_eq!(records.len(), 5);
}

#[tokio::test]
async fn given_a_failing_item_without_capture_when_mapped_then_the_run_fails() {
    let graph = mapped(per_item(OnFailure::Propagate)).unwrap();
    let outcome = run_parent(
        &graph,
        start(&graph, &["ok1", "bad1"]),
        &parent_config(1),
        Arc::new(NoopObserver),
    )
    .await;
    assert!(outcome.is_err());
}

#[tokio::test]
async fn given_a_call_body_capturing_with_a_set_when_an_item_fails_then_the_map_refuses_the_update()
{
    let capture = OnFailure::Capture(vec![CaptureUpdate::Set(
        key("result"),
        CaptureSource::Reason,
    )]);
    let graph = mapped(per_item(capture)).unwrap();
    let failure = run_parent(
        &graph,
        start(&graph, &["bad1"]),
        &parent_config(1),
        Arc::new(NoopObserver),
    )
    .await
    .err()
    .unwrap();
    let GraphError::NodeFailed {
        source: crate::error::NodeFault::Returned(returned),
        ..
    } = failure.error
    else {
        panic!("expected the map to fail");
    };
    assert!(matches!(
        returned.downcast_ref::<GraphError>(),
        Some(GraphError::MapBodyNotAppend { .. })
    ));
}

#[test]
fn given_a_capture_from_a_missing_source_or_into_a_target_of_another_kind_when_built_then_capture_mismatch()
 {
    for capture in [
        CaptureUpdate::Set(key("result"), CaptureSource::From(key("missing"))),
        CaptureUpdate::Set(key("result"), CaptureSource::Const(Value::int(1))),
        CaptureUpdate::Set(key("steps"), CaptureSource::Reason),
        CaptureUpdate::Set(key("missing"), CaptureSource::Reason),
        CaptureUpdate::Append(key("result"), CaptureSource::Reason),
        CaptureUpdate::Append(key("labels"), CaptureSource::From(key("count"))),
    ] {
        let call = picky_call().on_failure(OnFailure::Capture(vec![capture.clone()]));
        let error = crate::testkit::refusal(parent_with(call));
        assert!(
            matches!(error, GraphError::SubGraphCaptureMismatch { ref key, .. } if *key == *capture.target().key()),
            "{capture:?} gave {error}"
        );
    }
    let call = picky_call().on_failure(OnFailure::Capture(vec![
        CaptureUpdate::Append(key("counts"), CaptureSource::From(key("count"))),
        CaptureUpdate::Set(key("steps"), CaptureSource::Const(Value::int(-1))),
    ]));
    assert!(parent_with(call).is_ok());
}
