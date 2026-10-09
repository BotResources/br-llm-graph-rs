use std::sync::Arc;

use super::test_support::*;
use crate::error::GraphError;
use crate::graph::subgraph::{Input, Output, SubGraph};
use crate::graph::{Always, Graph, GraphBuilder, Map};
use crate::observe::NoopObserver;
use crate::run::Outcome;
use crate::state::Value;

/// One refinement per item: `text` from the item, the answer and the count
/// appended to `results` and `counts`.
fn per_item() -> SubGraph {
    SubGraph::call(refine_graph())
        .input(key("text"), Input::From(key("item")))
        .config(key("rounds"), Input::Config(key("rounds")))
        .output(key("answer"), Output::Append(key("results")))
        .output(key("count"), Output::Append(key("counts")))
}

fn mapped(body: SubGraph) -> Result<Graph, GraphError> {
    let map = Map {
        list: key("items"),
        item: key("item"),
        body: Box::new(body),
        max_concurrency: None,
        on_item_failure: crate::graph::ItemFailure::Finish,
    };
    GraphBuilder::new(parent_schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(nid("each"), Always(end("done")))
        .input(key("items"))
        .build()
}

#[test]
fn given_a_call_body_that_only_appends_when_built_then_ok() {
    assert!(mapped(per_item()).is_ok());
    assert!(mapped(per_item().output_end_label(Output::Append(key("labels")))).is_ok());
}

/// A call body that sets passes the build (the call cannot know it is a map
/// body) and is refused by the map at run time, never silently dropped.
#[tokio::test]
async fn given_a_call_body_with_a_set_output_when_mapped_then_the_map_refuses_the_update() {
    for body in [
        per_item().output(key("answer"), Output::Set(key("result"))),
        per_item().output_end_label(Output::Set(key("label"))),
    ] {
        let graph = mapped(body).unwrap();
        let items = Value::list(vec![Value::str("x")]);
        let state = graph.start_state([(key("items"), items)]).unwrap();
        let failure = run_parent(&graph, state, &parent_config(1), Arc::new(NoopObserver))
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
}

#[test]
fn given_a_call_body_appending_to_a_list_of_another_kind_when_built_then_target_mismatch() {
    let body = per_item().output(key("count"), Output::Append(key("results")));
    assert!(matches!(
        crate::testkit::refusal(mapped(body)),
        GraphError::SubGraphTargetMismatch { .. }
    ));
}

#[test]
fn given_a_call_body_with_an_unmapped_input_when_built_then_input_unmapped() {
    let body = SubGraph::call(refine_graph())
        .config(key("rounds"), Input::Config(key("rounds")))
        .output(key("answer"), Output::Append(key("results")));
    assert!(matches!(
        crate::testkit::refusal(mapped(body)),
        GraphError::SubGraphInputUnmapped { .. }
    ));
}

#[tokio::test]
async fn given_a_call_body_when_mapped_then_each_item_runs_the_graph_and_outputs_stay_aligned() {
    let graph = mapped(per_item().output_end_label(Output::Append(key("labels")))).unwrap();
    let items = Value::list(["x", "y", "z"].map(Value::str).to_vec());
    let state = graph.start_state([(key("items"), items)]).unwrap();
    let outcome = run_parent(&graph, state, &parent_config(2), Arc::new(NoopObserver)).await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    assert_eq!(
        state.list(&key("results")).unwrap(),
        &["x#2", "y#2", "z#2"].map(Value::str)
    );
    assert_eq!(
        state.list(&key("counts")).unwrap(),
        &[Value::int(2), Value::int(2), Value::int(2)]
    );
    assert_eq!(state.list(&key("labels")).unwrap().len(), 3);
}
