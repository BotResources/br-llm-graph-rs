//! The keys a function declares: checked against the graph of an LLM call,
//! declared by the graph of an agent.

use crate::error::GraphError;
use crate::react::given::reads::{Reads, declare_reads};
use crate::react::test_support::{ckey, key, skey};
use crate::state::{Kind, Schema};

fn graph_schema() -> Schema {
    Schema::builder()
        .state_by_name(key("topic"), Kind::Str)
        .state_by_name(key("rules"), Kind::list(Kind::Str))
        .config_by_name(key("width"), Kind::Int)
        .build()
}

#[test]
fn given_keys_declared_by_name_when_read_then_their_typed_keys() {
    let mut reads = Reads::new();
    let topic = reads.state::<String>("topic").unwrap();
    let width = reads.config::<i64>("width").unwrap();
    assert_eq!((topic.as_str(), width.as_str()), ("topic", "width"));
    let mut expected = Reads::new();
    expected.read_state(&topic);
    expected.read_config(&width);
    assert_eq!(reads, expected);
    assert!(matches!(
        Reads::new().state::<String>("Topic"),
        Err(GraphError::Identifier { field: "key", .. })
    ));
}

#[test]
fn given_reads_of_keys_the_graph_declares_with_their_kinds_when_checked_then_accepted() {
    let mut reads = Reads::new();
    reads.read_state(&skey::<String>("topic"));
    reads.read_state(&skey::<Vec<String>>("rules"));
    reads.read_config(&ckey::<i64>("width"));
    assert!(reads.check(&graph_schema()).is_ok());
}

#[test]
fn given_a_read_the_graph_does_not_declare_when_checked_then_unknown_key() {
    let mut reads = Reads::new();
    reads.read_config(&ckey::<String>("topic"));
    assert!(matches!(
        reads.check(&graph_schema()),
        Err(GraphError::UnknownKey { key: found }) if found == key("topic")
    ));
}

#[test]
fn given_a_read_of_another_kind_when_checked_then_kind_mismatch_naming_both_kinds() {
    let mut reads = Reads::new();
    reads.read_state(&skey::<String>("rules"));
    assert!(matches!(
        reads.check(&graph_schema()),
        Err(GraphError::KindMismatch { key: named, expected: Kind::Str, found: "list" })
            if named == key("rules")
    ));
    let mut reads = Reads::new();
    reads.read_config(&ckey::<String>("width"));
    assert!(matches!(
        reads.check(&graph_schema()),
        Err(GraphError::KindMismatch { key: named, expected: Kind::Str, found: "int" })
            if named == key("width")
    ));
}

#[test]
fn given_keys_read_by_several_functions_when_declared_then_each_once_inputs_in_order() {
    let mut first = Reads::new();
    first.read_state(&skey::<String>("persona"));
    first.read_config(&ckey::<String>("tone"));
    let mut second = Reads::new();
    second.read_state(&skey::<Vec<String>>("notes"));
    second.read_state(&skey::<String>("persona"));
    second.read_config(&ckey::<String>("tone"));
    let mut schema = Schema::builder()
        .state_by_name(key("answer"), Kind::Str)
        .build();

    let inputs = declare_reads(&mut schema, [&first, &second]).unwrap();

    assert_eq!(inputs, vec![key("persona"), key("notes")]);
    assert_eq!(schema.state.get(&key("persona")), Some(&Kind::Str));
    assert_eq!(
        schema.state.get(&key("notes")),
        Some(&Kind::list(Kind::Str))
    );
    assert_eq!(schema.config.get(&key("tone")), Some(&Kind::Str));
    assert_eq!(schema.config.len(), 1);
}

#[test]
fn given_a_key_the_graph_already_has_when_declared_then_key_collision() {
    for reads in [
        {
            let mut reads = Reads::new();
            reads.read_state(&skey::<String>("topic"));
            reads
        },
        {
            let mut reads = Reads::new();
            reads.read_config(&ckey::<i64>("width"));
            reads
        },
    ] {
        let mut schema = graph_schema();
        assert!(matches!(
            declare_reads(&mut schema, [&reads]),
            Err(GraphError::KeyCollision { .. })
        ));
    }
}

#[test]
fn given_one_key_declared_with_two_kinds_when_declared_then_key_collision() {
    let mut first = Reads::new();
    first.read_state(&skey::<String>("persona"));
    let mut second = Reads::new();
    second.read_state(&skey::<Vec<String>>("persona"));
    let mut schema = Schema::default();
    assert!(matches!(
        declare_reads(&mut schema, [&first, &second]),
        Err(GraphError::KeyCollision { key: found }) if found == key("persona")
    ));
    let mut alone = Reads::new();
    alone.read_config(&ckey::<String>("tone"));
    alone.read_config(&ckey::<i64>("tone"));
    let mut schema = Schema::default();
    assert!(matches!(
        declare_reads(&mut schema, [&alone]),
        Err(GraphError::KeyCollision { key: found }) if found == key("tone")
    ));
}

#[test]
fn given_a_state_key_and_a_configuration_key_of_one_name_when_declared_then_both() {
    let mut reads = Reads::new();
    reads.read_state(&skey::<String>("tone"));
    reads.read_config(&ckey::<String>("tone"));
    let mut schema = Schema::default();
    assert_eq!(
        declare_reads(&mut schema, [&reads]).unwrap(),
        vec![key("tone")]
    );
    assert_eq!(schema.config.get(&key("tone")), Some(&Kind::Str));
}
