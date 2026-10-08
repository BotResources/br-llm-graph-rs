use crate::error::GraphError;
use crate::graph::{Graph, GraphBuilder};
use crate::run::test_support::{edge, end, key, nid, noop_node, schema};
use crate::state::{Kind, Schema, Value};
use crate::value::Key;

fn build(schema: Schema, inputs: &[&str], outputs: &[&str]) -> Result<Graph, GraphError> {
    let mut builder = GraphBuilder::new(schema)
        .entry(nid("a"))
        .node(nid("a"), noop_node())
        .edge(nid("a"), edge(vec![end("done")]));
    for name in inputs {
        builder = builder.input(key(name));
    }
    for name in outputs {
        builder = builder.output(key(name));
    }
    builder.build()
}

#[test]
fn given_inputs_and_outputs_when_built_then_signature_lists_them_with_kinds() {
    let graph = build(schema(), &["items", "count"], &["outs"]).unwrap();
    let signature = graph.signature();
    let inputs: Vec<(&Key, &Kind)> = signature.inputs.iter().collect();
    assert_eq!(
        inputs,
        vec![
            (&key("count"), &Kind::Int),
            (&key("items"), &Kind::list(Kind::Str))
        ]
    );
    assert_eq!(
        signature.outputs.get(&key("outs")),
        Some(&Kind::list(Kind::Str))
    );
    assert_eq!(signature.outputs.len(), 1);
}

#[test]
fn given_no_declaration_when_built_then_signature_is_empty() {
    let graph = build(schema(), &[], &[]).unwrap();
    assert!(graph.signature().inputs.is_empty());
    assert!(graph.signature().outputs.is_empty());
}

#[test]
fn given_key_declared_input_and_output_when_built_then_allowed() {
    let graph = build(schema(), &["count"], &["count"]).unwrap();
    assert!(graph.signature().inputs.contains_key(&key("count")));
    assert!(graph.signature().outputs.contains_key(&key("count")));
}

#[test]
fn given_unknown_input_or_output_key_when_built_then_unknown_key() {
    assert!(matches!(
        build(schema(), &["ghost"], &[]),
        Err(GraphError::UnknownKey { key }) if key == self::key("ghost")
    ));
    assert!(matches!(
        build(schema(), &[], &["ghost"]),
        Err(GraphError::UnknownKey { .. })
    ));
}

#[test]
fn given_input_declared_twice_when_built_then_duplicate_input() {
    assert!(matches!(
        build(schema(), &["count", "count"], &[]),
        Err(GraphError::DuplicateInput { .. })
    ));
}

#[test]
fn given_output_declared_twice_when_built_then_duplicate_output() {
    assert!(matches!(
        build(schema(), &[], &["outs", "outs"]),
        Err(GraphError::DuplicateOutput { .. })
    ));
}

#[test]
fn given_explicit_default_of_the_wrong_kind_when_built_then_kind_mismatch() {
    let schema = Schema::builder()
        .state_with_default(key("count"), Kind::Int, Value::str("seven"))
        .build();
    assert!(matches!(
        build(schema, &[], &[]),
        Err(GraphError::KindMismatch { .. })
    ));
}

#[test]
fn given_default_on_an_undeclared_key_when_built_then_unknown_key() {
    let mut schema = schema();
    schema.defaults.insert(key("ghost"), Value::int(1));
    assert!(matches!(
        build(schema, &[], &[]),
        Err(GraphError::UnknownKey { .. })
    ));
}
