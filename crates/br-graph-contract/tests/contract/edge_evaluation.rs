use std::error::Error;

use br_graph_contract::{EdgeError, Graph, GraphBuilder, Target};

use crate::common::{Tally, edge, edge_answering, plain, state, targets};

fn fork_with(answer: Result<Vec<Target>, String>) -> Graph<Tally> {
    GraphBuilder::new("1")
        .node(plain("start"))
        .node(plain("alpha"))
        .node(plain("beta"))
        .node(plain("gamma"))
        .entry("start")
        .edge(edge_answering(
            "start",
            &["alpha", "beta", "gamma", "END"],
            answer,
        ))
        .edge(edge("alpha", &["END"]))
        .edge(edge("beta", &["END"]))
        .edge(edge("gamma", &["END"]))
        .build()
        .unwrap()
}

fn evaluate(answer: Result<Vec<Target>, String>) -> Result<Vec<Target>, EdgeError> {
    fork_with(answer).evaluate_edge("start", &state(0))
}

#[test]
fn given_a_declared_choice_when_evaluated_then_the_targets_come_back() {
    let result = evaluate(Ok(targets(&["beta"]))).unwrap();
    assert_eq!(result, targets(&["beta"]));
}

#[test]
fn given_a_choice_with_repeats_in_any_order_when_evaluated_then_it_is_deduplicated_in_declaration_order()
 {
    let result = evaluate(Ok(targets(&["gamma", "alpha", "gamma", "alpha"]))).unwrap();
    assert_eq!(result, targets(&["alpha", "gamma"]));
}

#[test]
fn given_end_repeated_when_evaluated_then_one_end_comes_back() {
    let result = evaluate(Ok(targets(&["END", "END"]))).unwrap();
    assert_eq!(result, vec![Target::End]);
}

#[test]
fn given_a_node_the_graph_does_not_hold_when_evaluated_then_it_is_unknown_node() {
    let error = fork_with(Ok(targets(&["END"])))
        .evaluate_edge("ghost", &state(0))
        .unwrap_err();
    assert!(matches!(error, EdgeError::UnknownNode { node } if node == "ghost"));
}

#[test]
fn given_a_target_that_is_not_declared_when_evaluated_then_it_is_refused() {
    let error = evaluate(Ok(targets(&["alpha", "delta"]))).unwrap_err();
    assert!(matches!(
        error,
        EdgeError::UndeclaredTarget { node, target } if node == "start" && target == Target::node("delta")
    ));
}

#[test]
fn given_a_node_named_end_when_evaluated_then_it_is_not_the_end_target() {
    let error = evaluate(Ok(vec![Target::Node("END".into())])).unwrap_err();
    assert!(matches!(error, EdgeError::UndeclaredTarget { .. }));
}

#[test]
fn given_a_declared_node_of_another_edge_when_evaluated_then_it_is_still_undeclared_here() {
    let graph = GraphBuilder::new("1")
        .node(plain("start"))
        .node(plain("other"))
        .entry("start")
        .entry("other")
        .edge(edge_answering("start", &["END"], Ok(targets(&["other"]))))
        .edge(edge("other", &["END"]))
        .build()
        .unwrap();
    let error = graph.evaluate_edge("start", &state(0)).unwrap_err();
    assert!(matches!(error, EdgeError::UndeclaredTarget { .. }));
}

#[test]
fn given_an_empty_list_when_evaluated_then_it_is_refused() {
    let error = evaluate(Ok(Vec::new())).unwrap_err();
    assert!(matches!(error, EdgeError::EmptyTargets { node } if node == "start"));
}

#[test]
fn given_end_mixed_with_nodes_when_evaluated_then_it_is_refused() {
    let error = evaluate(Ok(targets(&["alpha", "END"]))).unwrap_err();
    assert!(matches!(error, EdgeError::EndMixedWithNodes { node } if node == "start"));
}

#[test]
fn given_an_edge_that_fails_when_evaluated_then_the_opaque_cause_is_kept() {
    let error = evaluate(Err("state cannot be used".into())).unwrap_err();
    let EdgeError::Failed(cause) = &error else {
        unreachable!("{error:?}");
    };
    assert_eq!(cause.to_string(), "state cannot be used");
    assert!(error.source().is_some());
}

#[test]
fn given_the_same_state_when_evaluated_twice_then_the_answer_is_the_same() {
    let graph = fork_with(Ok(targets(&["gamma", "beta"])));
    let state = state(1);
    let first = graph.evaluate_edge("start", &state).unwrap();
    let second = graph.evaluate_edge("start", &state).unwrap();
    assert_eq!(first, second);
    assert_eq!(first, targets(&["beta", "gamma"]));
}
