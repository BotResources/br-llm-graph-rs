use std::collections::BTreeMap;

use super::*;
use crate::state::Value;

fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

fn schema() -> Schema {
    Schema::builder()
        .config(key("deep"), Kind::Bool)
        .config(key("width"), Kind::Int)
        .build()
}

fn config(deep: bool) -> Config {
    let mut values = BTreeMap::new();
    values.insert(key("deep"), Value::bool(deep));
    values.insert(key("width"), Value::int(2));
    Config::new(&schema(), values).unwrap()
}

#[test]
fn given_fixed_switch_when_resolved_then_its_value() {
    for value in [true, false] {
        let switch = Switch::Fixed(value);
        assert!(switch.check(&schema()).is_ok());
        assert_eq!(switch.resolve(&config(!value)).unwrap(), value);
    }
}

#[test]
fn given_bool_config_switch_when_resolved_then_the_config_value() {
    let switch = Switch::Config(key("deep"));
    assert!(switch.check(&schema()).is_ok());
    assert!(switch.resolve(&config(true)).unwrap());
    assert!(!switch.resolve(&config(false)).unwrap());
}

#[test]
fn given_config_switch_on_non_bool_or_undeclared_key_when_checked_then_refused() {
    for name in ["width", "missing"] {
        assert!(matches!(
            Switch::Config(key(name)).check(&schema()),
            Err(GraphError::SwitchKeyMismatch { key: found }) if found == key(name)
        ));
    }
}
