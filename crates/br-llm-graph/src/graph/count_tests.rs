use std::collections::BTreeMap;

use super::*;
use crate::state::Value;

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

fn schema() -> Schema {
    Schema::builder()
        .config(key("budget"), Kind::Int)
        .config(key("label"), Kind::Str)
        .build()
}

fn config(budget: i64) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("budget"), Value::int(budget));
    values.insert(key("label"), Value::str("x"));
    Config::new(&schema(), values).unwrap()
}

#[test]
fn given_fixed_count_when_resolved_then_its_value_zero_included() {
    for value in [0, 3] {
        let count = Count::Fixed(value);
        assert!(count.check(&schema()).is_ok());
        assert_eq!(count.resolve(&config(9)).unwrap(), value);
    }
}

#[test]
fn given_int_config_count_when_resolved_then_the_config_value_zero_included() {
    let count = Count::Config(key("budget"));
    assert!(count.check(&schema()).is_ok());
    for value in [0, 4] {
        assert_eq!(
            count.resolve(&config(value)).unwrap(),
            usize::try_from(value).unwrap()
        );
    }
}

#[test]
fn given_config_count_below_zero_when_resolved_then_refused() {
    let count = Count::Config(key("budget"));
    for value in [-1, -7] {
        assert!(matches!(
            count.resolve(&config(value)),
            Err(GraphError::CountNegative { value: found, .. }) if found == value
        ));
    }
}

#[test]
fn given_config_count_on_non_int_or_undeclared_key_when_checked_then_refused() {
    for name in ["label", "missing"] {
        assert!(matches!(
            Count::Config(key(name)).check(&schema()),
            Err(GraphError::CountKeyMismatch { key: found }) if found == key(name)
        ));
    }
}
