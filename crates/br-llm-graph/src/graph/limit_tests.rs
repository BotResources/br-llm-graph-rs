use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use super::*;
use crate::state::Value;

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

fn schema() -> Schema {
    Schema::builder()
        .config(key("width"), Kind::Int)
        .config(key("label"), Kind::Str)
        .build()
}

fn config(width: i64) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("width"), Value::int(width));
    values.insert(key("label"), Value::str("x"));
    Config::new(&schema(), values).unwrap()
}

#[test]
fn given_fixed_limit_when_resolved_then_its_value() {
    let limit = Limit::Fixed(NonZeroUsize::new(3).unwrap());
    assert!(limit.check(&schema()).is_ok());
    assert_eq!(limit.resolve(&config(9)).unwrap().get(), 3);
}

#[test]
fn given_int_config_limit_when_resolved_then_the_config_value() {
    let limit = Limit::Config(key("width"));
    assert!(limit.check(&schema()).is_ok());
    assert_eq!(limit.resolve(&config(4)).unwrap().get(), 4);
}

#[test]
fn given_config_limit_below_one_when_resolved_then_refused() {
    let limit = Limit::Config(key("width"));
    for value in [0, -2] {
        assert!(matches!(
            limit.resolve(&config(value)),
            Err(GraphError::LimitNotPositive { value: found, .. }) if found == value
        ));
    }
}

#[test]
fn given_config_limit_on_non_int_or_undeclared_key_when_checked_then_refused() {
    for name in ["label", "missing"] {
        assert!(matches!(
            Limit::Config(key(name)).check(&schema()),
            Err(GraphError::LimitKeyMismatch { .. })
        ));
    }
}
