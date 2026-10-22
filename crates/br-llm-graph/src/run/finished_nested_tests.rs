//! The node finished event of a call and of a map says what the node itself
//! did: a call whose child panics returns an error, while a panic in a map's
//! body is the map's. The nodes of a called graph finish under the calling
//! node's occurrence; the items of a map are not nodes of a superstep.

use std::sync::Arc;

use crate::error::{GraphError, NodeFault};
use crate::graph::{Always, GraphBuilder, ItemFailure, Map, SubGraph};
use crate::observe::NodeOutcome;
use crate::run::control::control;
use crate::run::runner::run;
use crate::run::test_support::*;
use crate::state::Value;
use crate::update::Update;

#[tokio::test]
async fn given_a_called_graph_whose_node_panics_when_run_then_the_node_panicked_and_the_call_returned_an_error()
 {
    let child = GraphBuilder::new(schema())
        .entry(nid("work"))
        .node(nid("work"), panicking_node())
        .edge(nid("work"), Always(end("done")))
        .build()
        .unwrap();
    let graph = GraphBuilder::new(schema())
        .entry(nid("call"))
        .node(nid("call"), SubGraph::call(Arc::new(child)))
        .edge(nid("call"), Always(end("done")))
        .build()
        .unwrap();
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
