//! The keys a function declares, checked against the graph of an LLM call
//! (an agent's graph declares them: see `declarations_tests.rs`).

use crate::error::GraphError;
use crate::react::given::reads::Reads;
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
