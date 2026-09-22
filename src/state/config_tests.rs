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
        .config(key("model"), Kind::Str)
        .config(key("limit"), Kind::Int)
        .build()
}

fn values() -> BTreeMap<Key, Value> {
    let mut values = BTreeMap::new();
    values.insert(key("model"), Value::str("scripted"));
    values.insert(key("limit"), Value::int(3));
    values
}

#[test]
fn given_complete_values_when_new_then_ok() {
    assert!(Config::new(&schema(), values()).is_ok());
}

#[test]
fn given_missing_config_key_when_new_then_missing() {
    let mut values = values();
    values.remove(&key("model"));
    assert!(matches!(
        Config::new(&schema(), values),
        Err(GraphError::MissingKey { .. })
    ));
}

#[test]
fn given_wrong_kind_when_new_then_mismatch() {
    let mut values = values();
    values.insert(key("limit"), Value::str("no"));
    assert!(matches!(
        Config::new(&schema(), values),
        Err(GraphError::KindMismatch { .. })
    ));
}

#[test]
fn given_config_when_round_tripped_then_identical() {
    let config = Config::new(&schema(), values()).unwrap();
    let json = serde_json::to_string(&config).unwrap();
    assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
}

#[test]
fn given_kind_mismatch_json_when_loaded_then_refused() {
    let config = Config::new(&schema(), values()).unwrap();
    let json = serde_json::to_string(&config).unwrap();
    let broken = json.replace("\"value\":3", "\"value\":\"three\"");
    assert!(serde_json::from_str::<Config>(&broken).is_err());
}

#[test]
fn given_getters_when_read_then_typed() {
    let config = Config::new(&schema(), values()).unwrap();
    assert_eq!(config.str(&key("model")).unwrap(), "scripted");
    assert_eq!(config.int(&key("limit")).unwrap(), 3);
}
