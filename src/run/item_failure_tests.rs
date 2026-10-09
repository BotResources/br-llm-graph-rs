use std::num::NonZeroUsize;
use std::sync::Arc;

use futures_util::future::join;

use crate::error::{GraphError, NodeFault};
use crate::graph::{CaptureSource, CaptureUpdate};
use crate::graph::{Context, Graph, GraphBuilder, ItemFailure, Limit, Map};
use crate::observe::NoopObserver;
use crate::run::Checkpoint;
use crate::run::gates::{Gates, PickyBody};
use crate::run::inbox::channel;
use crate::run::outcome::{Outcome, RunFailure};
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::{State, Value};
use crate::update::Update;

const ITEMS: [&str; 6] = ["a", "b", "c", "d", "e", "f"];

fn ctx() -> Context {
    crate::testkit::context(Arc::new(NoopObserver))
}

fn mapped(body: PickyBody, policy: ItemFailure) -> Result<Graph, GraphError> {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: Some(Limit::Fixed(NonZeroUsize::new(2).unwrap())),
        on_item_failure: policy,
    };
    GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), edge(vec![end("done")]))
        .build()
}

fn seeded() -> State {
    let mut state = base_state();
    state
        .apply_batch(&[Update::Set {
            key: key("items"),
            value: Value::list(ITEMS.map(Value::str).to_vec()),
        }])
        .unwrap();
    state
}

/// Runs the map once, releasing items in `order` as they start.
async fn run_releasing(
    graph: &Graph,
    gates: &Gates,
    order: &[&str],
) -> Result<Outcome, RunFailure> {
    let (_sender, mut inbox) = channel();
    let (ctx, config) = (ctx(), config());
    let running = run(graph, &config, seeded(), None, &ctx, &mut inbox);
    let driver = async {
        for item in order {
            gates.release(item).await;
        }
    };
    join(running, driver).await.0
}

/// Opens every gate, then resumes `checkpoint` to its end.
async fn resume_open(graph: &Graph, gates: &Gates, checkpoint: Checkpoint) -> State {
    for item in ITEMS {
        gates.open(item);
    }
    let (_sender, mut inbox) = channel();
    let resumed = ctx().with_pending(checkpoint.pending);
    let outcome = run(
        graph,
        &config(),
        checkpoint.state,
        Some(checkpoint.cursor),
        &resumed,
        &mut inbox,
    )
    .await;
    match outcome {
        Ok(Outcome::Finished { state, .. }) => state,
        Ok(_) => panic!("expected finished"),
        Err(failure) => panic!("expected finished, got {}", failure.error),
    }
}

fn returned_message(failure: &RunFailure) -> String {
    match &failure.error {
        GraphError::NodeFailed {
            source: NodeFault::Returned(error),
            ..
        } => error.to_string(),
        other => panic!("expected a returned error, got {other}"),
    }
}

fn pending(checkpoint: &Checkpoint) -> Vec<String> {
    checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[tokio::test(flavor = "current_thread")]
async fn given_fail_fast_with_width_two_when_an_item_fails_then_no_new_item_starts_and_running_ones_finish()
 {
    let gates = Gates::new(&ITEMS);
    let body = PickyBody::new(&gates, &["b"]);
    let failing = body.failing.clone();
    let graph = mapped(body, ItemFailure::FailFast).unwrap();
    let failure = run_releasing(&graph, &gates, &["b", "a"])
        .await
        .err()
        .unwrap();
    assert_eq!(gates.started(), strings(&["a", "b"]));
    assert_eq!(gates.finished(), strings(&["b", "a"]));
    assert_eq!(returned_message(&failure), "item b failed");
    assert_eq!(pending(&failure.checkpoint), vec!["m[0]"]);

    failing.lock().unwrap().clear();
    let state = resume_open(&graph, &gates, failure.checkpoint).await;
    assert_eq!(
        gates.started(),
        strings(&["a", "b", "b", "c", "d", "e", "f"])
    );
    assert_eq!(state.list(&key("outs")).unwrap(), &ITEMS.map(Value::str));
}

#[tokio::test(flavor = "current_thread")]
async fn given_fail_fast_when_two_running_items_fail_then_the_first_in_item_order_is_reported() {
    let gates = Gates::new(&ITEMS);
    let graph = mapped(PickyBody::new(&gates, &["a", "b"]), ItemFailure::FailFast).unwrap();
    let failure = run_releasing(&graph, &gates, &["b", "a"])
        .await
        .err()
        .unwrap();
    assert_eq!(gates.started(), strings(&["a", "b"]));
    assert_eq!(returned_message(&failure), "item a failed");
    assert!(failure.checkpoint.pending.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn given_finish_when_an_item_fails_then_every_item_runs_and_a_resume_runs_only_the_failed_one()
 {
    let gates = Gates::new(&ITEMS);
    let body = PickyBody::new(&gates, &["b"]);
    let failing = body.failing.clone();
    let graph = mapped(body, ItemFailure::Finish).unwrap();
    let failure = run_releasing(&graph, &gates, &["b", "a", "c", "d", "e", "f"])
        .await
        .err()
        .unwrap();
    assert_eq!(gates.started(), strings(&ITEMS));
    assert_eq!(returned_message(&failure), "item b failed");
    assert_eq!(
        pending(&failure.checkpoint),
        vec!["m[0]", "m[2]", "m[3]", "m[4]", "m[5]"]
    );

    failing.lock().unwrap().clear();
    let state = resume_open(&graph, &gates, failure.checkpoint).await;
    assert_eq!(gates.started().len(), 7);
    assert_eq!(gates.started().last().map(String::as_str), Some("b"));
    assert_eq!(state.list(&key("outs")).unwrap(), &ITEMS.map(Value::str));
}

fn capture_item_and_reason() -> ItemFailure {
    ItemFailure::Capture(vec![
        CaptureUpdate::Append(key("outs"), CaptureSource::From(key("item"))),
        CaptureUpdate::Append(key("log"), CaptureSource::Reason),
    ])
}

#[tokio::test(flavor = "current_thread")]
async fn given_capture_when_items_fail_then_the_map_completes_with_failures_in_item_order_in_aligned_lists()
 {
    let gates = Gates::new(&ITEMS);
    let graph = mapped(
        PickyBody::new(&gates, &["b", "d"]),
        capture_item_and_reason(),
    )
    .unwrap();
    let outcome = run_releasing(&graph, &gates, &["b", "a", "d", "c", "f", "e"]).await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("a captured item failure must not fail the map");
    };
    assert_eq!(gates.started(), strings(&ITEMS));
    assert_eq!(state.list(&key("outs")).unwrap(), &ITEMS.map(Value::str));
    assert_eq!(
        state.list(&key("log")).unwrap(),
        &["ok", "item b failed", "ok", "item d failed", "ok", "ok"].map(Value::str)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn given_capture_when_an_item_panics_then_the_map_fails_and_a_resume_skips_the_recorded_items()
 {
    let gates = Gates::new(&ITEMS);
    let body = PickyBody::new(&gates, &["b"]);
    let panicking = body.panicking.clone();
    *panicking.lock().unwrap() = Some("c".to_owned());
    let graph = mapped(body, capture_item_and_reason()).unwrap();
    let (_sender, mut inbox) = channel();
    let (ctx, config) = (ctx(), config());
    let running = run(&graph, &config, seeded(), None, &ctx, &mut inbox);
    let driver = async {
        gates.release("a").await;
        gates.release("b").await;
        gates.until(|g| g.started().len() == 4).await;
        gates.open("c");
    };
    let (outcome, ()) = join(running, driver).await;
    let failure = outcome.err().unwrap();
    assert!(matches!(
        failure.error,
        GraphError::NodeFailed {
            source: NodeFault::Panic(_),
            ..
        }
    ));
    assert_eq!(pending(&failure.checkpoint), vec!["m[0]", "m[1]"]);

    *panicking.lock().unwrap() = None;
    let state = resume_open(&graph, &gates, failure.checkpoint).await;
    assert_eq!(
        gates.started(),
        strings(&["a", "b", "c", "d", "c", "d", "e", "f"])
    );
    assert_eq!(state.list(&key("outs")).unwrap(), &ITEMS.map(Value::str));
    assert_eq!(
        state.list(&key("log")).unwrap(),
        &["ok", "item b failed", "ok", "ok", "ok", "ok"].map(Value::str)
    );
}

#[test]
fn given_capture_updates_that_set_or_do_not_fit_when_built_then_refused() {
    let gates = Gates::new(&ITEMS);
    let set = ItemFailure::Capture(vec![CaptureUpdate::Set(key("out"), CaptureSource::Reason)]);
    assert!(matches!(
        crate::testkit::refusal(mapped(PickyBody::new(&gates, &[]), set)),
        GraphError::MapBodySet { .. }
    ));
    for capture in [
        CaptureUpdate::Append(key("outs"), CaptureSource::From(key("count"))),
        CaptureUpdate::Append(key("outs"), CaptureSource::From(key("missing"))),
        CaptureUpdate::Append(key("count"), CaptureSource::Reason),
        CaptureUpdate::Append(key("outs"), CaptureSource::Const(Value::int(1))),
    ] {
        let policy = ItemFailure::Capture(vec![capture.clone()]);
        assert!(
            matches!(
                crate::testkit::refusal(mapped(PickyBody::new(&gates, &[]), policy)),
                GraphError::CaptureMismatch { .. }
            ),
            "{capture:?} was accepted"
        );
    }
}
