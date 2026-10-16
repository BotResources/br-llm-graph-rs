use std::collections::BTreeMap;

use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::{Config, ConfigKey, Kind, Schema, State, StateKey, Value};
use crate::value::Key;

/// The state of `schema` at its start values.
fn start(schema: &Schema) -> State {
    let values = schema
        .state
        .keys()
        .map(|key| (key.clone(), schema.default_value(key).unwrap()))
        .collect();
    State::new(schema.clone(), values).unwrap()
}

#[test]
fn given_a_key_of_each_kind_when_read_at_start_then_the_neutral_value_as_its_type() {
    let mut schema = Schema::builder();
    let count: StateKey<i64> = schema.state("count").unwrap();
    let ratio: StateKey<f64> = schema.state("ratio").unwrap();
    let text: StateKey<String> = schema.state("text").unwrap();
    let flag: StateKey<bool> = schema.state("flag").unwrap();
    let items: StateKey<Vec<String>> = schema.state("items").unwrap();
    let chat: StateKey<Conversation> = schema.state("chat").unwrap();
    let state = start(&schema.build());

    let count: i64 = state.get(&count).unwrap();
    let ratio: f64 = state.get(&ratio).unwrap();
    let text: &str = state.get(&text).unwrap();
    let flag: bool = state.get(&flag).unwrap();
    let items: Vec<&str> = state.get(&items).unwrap();
    let chat: &Conversation = state.get(&chat).unwrap();
    assert_eq!((count, ratio, text, flag), (0, 0.0, "", false));
    assert!(items.is_empty());
    assert!(chat.entries().is_empty());
}

#[test]
fn given_typed_declarations_when_built_then_the_schema_holds_their_kinds() {
    let mut schema = Schema::builder();
    schema.state::<Vec<Vec<f64>>>("grid").unwrap();
    schema.state::<Conversation>("chat").unwrap();
    schema.config::<bool>("strict").unwrap();
    let schema = schema.build();
    assert_eq!(
        schema.state,
        BTreeMap::from([
            (key("grid"), Kind::list(Kind::list(Kind::Float))),
            (key("chat"), Kind::Conversation),
        ])
    );
    assert_eq!(schema.config, BTreeMap::from([(key("strict"), Kind::Bool)]));
}

#[test]
fn given_a_typed_default_when_read_at_start_then_the_default() {
    let mut schema = Schema::builder();
    let tone = schema
        .state_with_default("tone", "plain".to_owned())
        .unwrap();
    let limits = schema.state_with_default("limits", vec![1_i64, 2]).unwrap();
    let state = start(&schema.build());
    assert_eq!(state.get(&tone).unwrap(), "plain");
    assert_eq!(state.get(&limits).unwrap(), vec![1, 2]);
}

#[test]
fn given_a_float_default_that_is_not_finite_when_declared_then_float_not_finite() {
    let mut schema = Schema::builder();
    assert!(matches!(
        schema.state_with_default("ratio", f64::NAN),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        schema.state_with_default("ratios", vec![0.5, f64::INFINITY]),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(schema.build().state.is_empty());
}

#[test]
fn given_a_name_that_is_not_an_identifier_when_declared_then_identifier() {
    let mut schema = Schema::builder();
    assert!(matches!(
        schema.state::<i64>("Count"),
        Err(GraphError::Identifier { field: "key", .. })
    ));
    assert!(matches!(
        schema.config::<String>(""),
        Err(GraphError::Identifier { field: "key", .. })
    ));
    assert!(matches!(
        schema.state_with_default("a-b", 1_i64),
        Err(GraphError::Identifier { field: "key", .. })
    ));
}

#[test]
fn given_a_typed_key_the_state_does_not_declare_when_read_then_unknown_key() {
    let mut other = Schema::builder();
    let stray: StateKey<i64> = other.state("stray").unwrap();
    let mut schema = Schema::builder();
    schema.state::<i64>("count").unwrap();
    let state = start(&schema.build());
    assert!(matches!(
        state.get(&stray),
        Err(GraphError::UnknownKey { ref key }) if key.as_str() == "stray"
    ));
}

#[test]
fn given_a_name_the_state_declares_with_another_kind_when_read_then_kind_mismatch() {
    let mut other = Schema::builder();
    let count: StateKey<String> = other.state("count").unwrap();
    let mut schema = Schema::builder();
    schema.state::<i64>("count").unwrap();
    let state = start(&schema.build());
    match state.get(&count) {
        Err(GraphError::KindMismatch {
            key,
            expected,
            found,
        }) => {
            assert_eq!(key.as_str(), "count");
            assert_eq!(expected, Kind::Str);
            assert_eq!(found, "int");
        }
        other => panic!("expected KindMismatch, got {other:?}"),
    }
}

#[test]
fn given_an_empty_list_of_another_element_kind_when_read_then_kind_mismatch() {
    let mut other = Schema::builder();
    let items: StateKey<Vec<String>> = other.state("items").unwrap();
    let mut schema = Schema::builder();
    schema.state::<Vec<i64>>("items").unwrap();
    let state = start(&schema.build());
    assert!(matches!(
        state.get(&items),
        Err(GraphError::KindMismatch { ref expected, .. }) if *expected == Kind::list(Kind::Str)
    ));
}

#[test]
fn given_a_name_declared_again_with_another_kind_when_read_through_the_first_key_then_kind_mismatch()
 {
    let mut schema = Schema::builder();
    let first: StateKey<i64> = schema.state("slot").unwrap();
    let second: StateKey<bool> = schema.state("slot").unwrap();
    let state = start(&schema.build());
    assert!(state.get(&second).is_ok());
    assert!(matches!(
        state.get(&first),
        Err(GraphError::KindMismatch { .. })
    ));
}

#[test]
fn given_typed_configuration_keys_when_read_then_their_values_as_their_types() {
    let mut schema = Schema::builder();
    let model: ConfigKey<String> = schema.config("model").unwrap();
    let width: ConfigKey<i64> = schema.config("width").unwrap();
    let tags: ConfigKey<Vec<String>> = schema.config("tags").unwrap();
    let schema = schema.build();
    let config = Config::new(
        &schema,
        BTreeMap::from([
            (key("model"), Value::str("small")),
            (key("width"), Value::int(4)),
            (key("tags"), Value::list(vec![Value::str("a")])),
        ]),
    )
    .unwrap();
    assert_eq!(config.get(&model).unwrap(), "small");
    assert_eq!(config.get(&width).unwrap(), 4);
    assert_eq!(config.get(&tags).unwrap(), vec!["a"]);
}

#[test]
fn given_a_configuration_key_of_another_schema_when_read_then_unknown_key_or_kind_mismatch() {
    let mut other = Schema::builder();
    let stray: ConfigKey<i64> = other.config("stray").unwrap();
    let width: ConfigKey<String> = other.config("width").unwrap();
    let mut schema = Schema::builder();
    schema.config::<i64>("width").unwrap();
    let config = Config::new(
        &schema.build(),
        BTreeMap::from([(key("width"), Value::int(4))]),
    )
    .unwrap();
    assert!(matches!(
        config.get(&stray),
        Err(GraphError::UnknownKey { .. })
    ));
    assert!(matches!(
        config.get(&width),
        Err(GraphError::KindMismatch { found: "int", .. })
    ));
}

#[test]
fn given_a_state_key_and_a_configuration_key_of_one_name_when_read_then_each_its_own() {
    let mut schema = Schema::builder();
    let state_model: StateKey<String> = schema.state("model").unwrap();
    let config_model: ConfigKey<i64> = schema.config("model").unwrap();
    let schema = schema.build();
    let state = start(&schema);
    let config = Config::new(&schema, BTreeMap::from([(key("model"), Value::int(7))])).unwrap();
    assert_eq!(state.get(&state_model).unwrap(), "");
    assert_eq!(config.get(&config_model).unwrap(), 7);
}

#[test]
fn given_a_typed_key_when_converted_then_its_name_and_its_kind_in_debug() {
    let mut schema = Schema::builder();
    let notes: StateKey<Vec<String>> = schema.state("notes").unwrap();
    assert_eq!(notes.as_str(), "notes");
    assert_eq!(notes.to_string(), "notes");
    assert_eq!(Key::from(&notes), key("notes"));
    assert_eq!(notes.clone(), notes);
    assert_eq!(
        format!("{notes:?}"),
        r#"StateKey { key: Key("notes"), kind: List { element: Str } }"#
    );
}

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}
