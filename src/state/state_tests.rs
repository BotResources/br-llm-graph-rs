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
        .state(key("count"), Kind::Int)
        .state(key("items"), Kind::list(Kind::Str))
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
