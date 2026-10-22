//! The node finished event of a call and of a map says what the node itself
//! did: a call whose child fails returns an error, or its capture updates
//! under `OnFailure::Capture`, while a panic in a map's body is the map's.
//! The nodes of a called graph finish under the calling node's occurrence;
//! the items of a map are not nodes of a superstep.

use std::sync::Arc;

use crate::error::{GraphError, NodeFault};
use crate::graph::{
    Always, CaptureSource, CaptureUpdate, Graph, GraphBuilder, ItemFailure, Map, Node, OnFailure,
    SubGraph,
};
use crate::observe::NodeOutcome;
use crate::run::control::control;
use crate::run::outcome::Outcome;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::Value;
use crate::update::Update;

/// A graph of one node, `work`, which ends with `done`.
fn child(work: impl Node + 'static) -> Arc<Graph> {
    let graph = GraphBuilder::new(schema())
        .entry(nid("work"))
        .node(nid("work"), work)
        .edge(nid("work"), Always(end("done")))
        .build()
        .unwrap();
    Arc::new(graph)
}

/// A graph of one node, `call`, which runs `call` and ends with `done`.
fn calling(call: SubGraph) -> Graph {
    GraphBuilder::new(schema())
        .entry(nid("call"))
        .node(nid("call"), call)
        .edge(nid("call"), Always(end("done")))
        .build()
        .unwrap()
}

#[tokio::test]
async fn given_a_called_graph_whose_node_panics_when_run_then_the_node_panicked_and_the_call_returned_an_error()
 {
    let graph = calling(SubGraph::call(child(panicking_node())));
    let finishes = Arc::new(Finishes::default());
    let ctx = crate::testkit::context(finishes.clone());
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), base_state(), None, &ctx, &mut channel)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        GraphError::NodeFailed {
            source: NodeFault::Returned(_),
            ..
        }
    ));
    assert_eq!(
        finishes.seen(),
        finished(&[
            ("call|work", NodeOutcome::Panic),
            ("|call", NodeOutcome::Error),
        ])
    );
}

#[tokio::test]
async fn given_a_called_graph_that_fails_under_capture_when_run_then_the_call_finished_with_its_capture_updates()
 {
    let capture = OnFailure::Capture(vec![CaptureUpdate::Set(key("out"), CaptureSource::Reason)]);
    let graph = calling(SubGraph::call(child(failing_node())).on_failure(capture));
    let finishes = Arc::new(Finishes::default());
    let ctx = crate::testkit::context(finishes.clone());
    let (_sender, mut channel) = control();
    let outcome = run(&graph, &config(), base_state(), None, &ctx, &mut channel).await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert!(state.str(&key("out")).unwrap().contains("boom"));
    assert_eq!(
        finishes.seen(),
        finished(&[
            ("call|work", NodeOutcome::Error),
            ("|call", NodeOutcome::Updates),
        ])
    );
}

#[tokio::test]
async fn given_a_map_whose_body_panics_when_run_then_the_map_panicked() {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(panicking_node()),
        max_concurrency: None,
        on_item_failure: ItemFailure::Finish,
    };
    let graph = GraphBuilder::new(schema())
        .entry(nid("m"))
        .map(nid("m"), map)
        .edge(nid("m"), Always(end("done")))
        .build()
        .unwrap();
    let mut state = base_state();
    state
        .apply_batch(&[Update::Set {
            key: key("items"),
            value: Value::list(vec![Value::str("a"), Value::str("b")]),
        }])
        .unwrap();
    let finishes = Arc::new(Finishes::default());
    let ctx = crate::testkit::context(finishes.clone());
    let (_sender, mut channel) = control();
    let failure = run(&graph, &config(), state, None, &ctx, &mut channel)
        .await
        .err()
        .unwrap();
    assert!(matches!(
        failure.error,
        GraphError::NodeFailed {
            source: NodeFault::Panic(_),
            ..
        }
    ));
    assert_eq!(finishes.seen(), finished(&[("|m", NodeOutcome::Panic)]));
}
