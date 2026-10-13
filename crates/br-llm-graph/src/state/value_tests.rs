use super::*;

#[test]
fn given_nan_when_float_then_refused() {
    assert!(matches!(
        Value::float(f64::NAN),
        Err(GraphError::FloatNotFinite)
    ));
    assert!(matches!(
        Value::float(f64::INFINITY),
        Err(GraphError::FloatNotFinite)
    ));
}

#[test]
fn given_nan_json_when_deserialized_then_refused() {
    let json = "{ \"type\": \"float\", \"value\": null }";
    assert!(serde_json::from_str::<Value>(json).is_err());
}

#[test]
fn given_scalar_values_when_round_tripped_then_identical() {
    for value in [
        Value::int(7),
        Value::float(1.5).unwrap(),
        Value::str("hi"),
        Value::bool(true),
    ] {
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), value);
    }
}

#[test]
fn given_int_value_when_serialized_then_tagged_with_value() {
    assert_eq!(
        serde_json::to_value(Value::int(3)).unwrap(),
        serde_json::json!({ "type": "int", "value": 3 })
    );
}

#[test]
fn given_list_of_ints_when_matches_list_int_then_true() {
    let value = Value::list(vec![Value::int(1), Value::int(2)]);
    assert!(value.matches(&Kind::list(Kind::Int)));
    assert!(!value.matches(&Kind::list(Kind::Str)));
    assert!(!value.matches(&Kind::Int));
}

#[test]
fn given_empty_list_when_matches_any_list_then_true() {
    assert!(Value::list(Vec::new()).matches(&Kind::list(Kind::Str)));
}

#[test]
fn given_mixed_list_when_matches_then_false() {
    let value = Value::list(vec![Value::int(1), Value::str("x")]);
    assert!(!value.matches(&Kind::list(Kind::Int)));
}
