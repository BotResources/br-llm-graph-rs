use std::collections::BTreeMap;

use super::*;
use crate::state::kind::Kind;
use crate::state::schema::Schema;
use crate::state::value::Value;
use crate::value::Key;

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

fn schema() -> Schema {
    Schema::builder()
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("items"), Kind::list(Kind::Str))
        .build()
}

fn values() -> BTreeMap<Key, Value> {
    let mut values = BTreeMap::new();
    values.insert(key("count"), Value::int(0));
    values.insert(key("items"), Value::list(Vec::new()));
    values
}

#[test]
fn given_complete_values_when_new_then_ok() {
    assert!(State::new(schema(), values()).is_ok());
}

#[test]
fn given_missing_key_when_new_then_missing_key() {
    let mut values = values();
    values.remove(&key("items"));
    assert!(matches!(
        State::new(schema(), values),
        Err(GraphError::MissingKey { .. })
    ));
}

#[test]
fn given_undeclared_key_when_new_then_unknown_key() {
    let mut values = values();
    values.insert(key("stray"), Value::int(1));
    assert!(matches!(
        State::new(schema(), values),
        Err(GraphError::UnknownKey { .. })
    ));
}

#[test]
fn given_wrong_kind_when_new_then_kind_mismatch() {
    let mut values = values();
    values.insert(key("count"), Value::str("no"));
    assert!(matches!(
        State::new(schema(), values),
        Err(GraphError::KindMismatch { .. })
    ));
}

#[test]
fn given_valid_state_when_round_tripped_then_identical() {
    let state = State::new(schema(), values()).unwrap();
    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<State>(&json).unwrap(), state);
}

#[test]
fn given_kind_mismatch_json_when_loaded_then_refused() {
    let state = State::new(schema(), values()).unwrap();
    let json = serde_json::to_string(&state).unwrap();
    let broken = json.replace("\"value\":0", "\"value\":\"oops\"");
    assert!(serde_json::from_str::<State>(&broken).is_err());
}

#[test]
fn given_wrong_schema_version_when_loaded_then_refused() {
    let state = State::new(schema(), values()).unwrap();
    let json = serde_json::to_string(&state).unwrap();
    let broken = json.replace("br-llm-graph/1", "br-llm-graph/2");
    assert!(serde_json::from_str::<State>(&broken).is_err());
}

#[test]
fn given_derive_when_key_declared_then_only_that_key_changes() {
    let state = State::new(schema(), values()).unwrap();
    let derived = state.derive(&key("count"), Value::int(9)).unwrap();
    assert_eq!(derived.int(&key("count")).unwrap(), 9);
    assert_eq!(state.int(&key("count")).unwrap(), 0);
}

#[test]
fn given_derive_with_wrong_kind_when_called_then_refused() {
    let state = State::new(schema(), values()).unwrap();
    assert!(matches!(
        state.derive(&key("count"), Value::str("no")),
        Err(GraphError::KindMismatch { .. })
    ));
}

#[test]
fn given_getters_when_wrong_kind_then_mismatch() {
    let state = State::new(schema(), values()).unwrap();
    assert!(matches!(
        state.str(&key("count")),
        Err(GraphError::KindMismatch { .. })
    ));
}

#[test]
fn given_an_undeclared_key_when_read_then_unknown_key_for_every_typed_read() {
    let state = State::new(schema(), values()).unwrap();
    let stray = key("stray");
    let reads = [
        state.int(&stray).err(),
        state.float(&stray).err(),
        state.str(&stray).err(),
        state.bool(&stray).err(),
        state.list(&stray).err(),
        state.conversation(&stray).err(),
        state.get(&stray).err(),
    ];
    for error in reads {
        assert!(
            matches!(error, Some(GraphError::UnknownKey { ref key }) if *key == stray),
            "{error:?}"
        );
    }
}

#[test]
fn given_a_key_that_is_not_a_list_when_read_as_a_list_then_a_list_is_the_expected_kind() {
    let state = State::new(schema(), values()).unwrap();
    let error = state.list(&key("count")).unwrap_err();
    assert!(
        matches!(
            error,
            GraphError::KindMismatch { ref key, expected: Kind::List { .. }, found: "int" }
                if *key == self::key("count")
        ),
        "{error:?}"
    );
}
