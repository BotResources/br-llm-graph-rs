use super::test_support::*;
use crate::error::GraphError;
use crate::graph::subgraph::{Input, Output, SubGraph};
use crate::graph::{Always, GraphBuilder};
use crate::state::Value;
use crate::value::Key;

fn bare() -> SubGraph {
    SubGraph::call(refine_graph())
}

fn with_input(source: Input) -> SubGraph {
    bare()
        .input(key("text"), source)
        .config(key("rounds"), Input::Config(key("rounds")))
}

fn with_config(source: Input) -> SubGraph {
    bare()
        .input(key("text"), Input::From(key("question")))
        .config(key("rounds"), source)
}

fn refused(call: SubGraph) -> GraphError {
    crate::testkit::refusal(parent_with(call))
}

fn names(error: &GraphError) -> Option<Key> {
    match error {
        GraphError::SubGraphNotAnInput { key, .. }
        | GraphError::SubGraphInputTwice { key, .. }
        | GraphError::SubGraphInputUnmapped { key, .. }
        | GraphError::SubGraphSourceMismatch { key, .. }
        | GraphError::SubGraphNotAConfig { key, .. }
        | GraphError::SubGraphConfigTwice { key, .. }
        | GraphError::SubGraphConfigUnmapped { key, .. }
        | GraphError::SubGraphConfigMismatch { key, .. }
        | GraphError::SubGraphNotAnOutput { key, .. }
        | GraphError::SubGraphTargetMismatch { key, .. } => Some(key.clone()),
        _ => None,
    }
}

#[test]
fn given_a_complete_call_when_built_then_ok() {
    assert!(parent_with(refine_call()).is_ok());
}

#[test]
fn given_a_call_registered_as_a_plain_node_when_built_then_it_is_checked_too() {
    let result = GraphBuilder::new(parent_schema())
        .entry(nid("ask"))
        .node(nid("ask"), bare())
        .edge(nid("ask"), Always(end("done")))
        .build();
    assert!(matches!(
        result,
        Err(GraphError::InvalidNode { ref node, ref source })
            if *node == nid("ask") && matches!(**source, GraphError::SubGraphInputUnmapped { .. })
    ));
}

#[test]
fn given_a_declared_input_left_unmapped_when_built_then_input_unmapped() {
    let error = refused(bare().config(key("rounds"), Input::Config(key("rounds"))));
    assert!(matches!(error, GraphError::SubGraphInputUnmapped { .. }));
    assert_eq!(names(&error), Some(key("text")));
}

#[test]
fn given_an_input_mapped_twice_when_built_then_input_twice() {
    let call = with_input(Input::From(key("question")))
        .input(key("text"), Input::Const(Value::str("again")));
    assert!(matches!(
        refused(call),
        GraphError::SubGraphInputTwice { .. }
    ));
}

#[test]
fn given_a_private_key_mapped_as_an_input_when_built_then_not_an_input() {
    let call =
        with_input(Input::From(key("question"))).input(key("count"), Input::From(key("count")));
    let error = refused(call);
    assert!(matches!(error, GraphError::SubGraphNotAnInput { .. }));
    assert_eq!(names(&error), Some(key("count")));
}

#[test]
fn given_a_source_missing_or_of_another_kind_when_built_then_source_mismatch() {
    for source in [
        Input::From(key("missing")),
        Input::From(key("count")),
        Input::Config(key("rounds")),
        Input::Config(key("missing")),
        Input::Const(Value::int(1)),
    ] {
        let error = refused(with_input(source.clone()));
        assert!(
            matches!(error, GraphError::SubGraphSourceMismatch { .. }),
            "{source:?} gave {error}"
        );
    }
    assert!(parent_with(with_input(Input::Config(key("tone")))).is_ok());
}

#[test]
fn given_a_child_configuration_key_left_unmapped_when_built_then_config_unmapped() {
    let call = bare().input(key("text"), Input::From(key("question")));
    let error = refused(call);
    assert!(matches!(error, GraphError::SubGraphConfigUnmapped { .. }));
    assert_eq!(names(&error), Some(key("rounds")));
}

#[test]
fn given_a_configuration_key_mapped_twice_when_built_then_config_twice() {
    let call =
        with_config(Input::Const(Value::int(1))).config(key("rounds"), Input::Const(Value::int(2)));
    assert!(matches!(
        refused(call),
        GraphError::SubGraphConfigTwice { .. }
    ));
}

#[test]
fn given_a_key_the_child_configuration_lacks_when_built_then_not_a_config() {
    let call =
        with_config(Input::Const(Value::int(1))).config(key("tone"), Input::Config(key("tone")));
    assert!(matches!(
        refused(call),
        GraphError::SubGraphNotAConfig { .. }
    ));
}

#[test]
fn given_a_configuration_source_missing_or_of_another_kind_when_built_then_config_mismatch() {
    for source in [
        Input::Config(key("tone")),
        Input::Config(key("missing")),
        Input::From(key("question")),
        Input::Const(Value::str("two")),
    ] {
        let error = refused(with_config(source.clone()));
        assert!(
            matches!(error, GraphError::SubGraphConfigMismatch { .. }),
            "{source:?} gave {error}"
        );
    }
    assert!(parent_with(with_config(Input::From(key("count")))).is_ok());
}

#[test]
fn given_a_key_that_is_not_a_declared_output_when_built_then_not_an_output() {
    for child_key in ["flag", "text", "missing"] {
        let call = refine_call().output(key(child_key), Output::Set(key("flag")));
        let error = refused(call);
        assert!(matches!(error, GraphError::SubGraphNotAnOutput { .. }));
        assert_eq!(names(&error), Some(key(child_key)));
    }
}

#[test]
fn given_a_target_missing_or_unable_to_take_the_output_when_built_then_target_mismatch() {
    for (child_key, target) in [
        ("answer", Output::Set(key("missing"))),
        ("answer", Output::Set(key("steps"))),
        ("answer", Output::Append(key("result"))),
        ("count", Output::Append(key("results"))),
        ("answer", Output::Append(key("missing"))),
    ] {
        let call = refine_call().output(key(child_key), target.clone());
        let error = refused(call);
        assert!(
            matches!(error, GraphError::SubGraphTargetMismatch { .. }),
            "{target:?} gave {error}"
        );
        assert_eq!(names(&error), Some(target.key().clone()));
    }
}

#[test]
fn given_an_end_label_target_that_is_not_text_when_built_then_target_mismatch() {
    for target in [Output::Set(key("steps")), Output::Append(key("results"))] {
        let call = refine_call().output_end_label(target.clone());
        match target {
            Output::Set(_) => assert!(matches!(
                refused(call),
                GraphError::SubGraphTargetMismatch { .. }
            )),
            Output::Append(_) => assert!(parent_with(call).is_ok()),
        }
    }
}
