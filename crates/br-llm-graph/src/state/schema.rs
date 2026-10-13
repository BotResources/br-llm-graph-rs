use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::value::Value;
use crate::value::Key;

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Schema {
    pub state: BTreeMap<Key, Kind>,
    pub config: BTreeMap<Key, Kind>,
    /// Explicit start values of state keys; a state key without one starts
    /// with the neutral value of its kind.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub defaults: BTreeMap<Key, Value>,
}

impl Schema {
    pub fn new(state: BTreeMap<Key, Kind>, config: BTreeMap<Key, Kind>) -> Self {
        Self {
            state,
            config,
            defaults: BTreeMap::new(),
        }
    }

    pub fn builder() -> SchemaBuilder {
        SchemaBuilder::default()
    }

    /// The start value of a state key: its explicit default, else the
    /// neutral value of its kind. `None` for a key that is not declared.
    pub fn default_value(&self, key: &Key) -> Option<Value> {
        match self.defaults.get(key) {
            Some(value) => Some(value.clone()),
            None => self.state.get(key).map(Kind::neutral),
        }
    }

    /// Every explicit default names a declared state key and has its kind.
    pub fn check_defaults(&self) -> Result<(), GraphError> {
        for (key, value) in &self.defaults {
            match self.state.get(key) {
                None => return Err(GraphError::UnknownKey { key: key.clone() }),
                Some(kind) if !value.matches(kind) => {
                    return Err(GraphError::KindMismatch {
                        key: key.clone(),
                        expected: kind.clone(),
                        found: value.tag(),
                    });
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct SchemaBuilder {
    state: BTreeMap<Key, Kind>,
    config: BTreeMap<Key, Kind>,
    defaults: BTreeMap<Key, Value>,
}

impl SchemaBuilder {
    pub fn state(mut self, key: Key, kind: Kind) -> Self {
        self.state.insert(key, kind);
        self
    }

    /// Declares a state key with an explicit start value. The value must have
    /// the kind of the key; `GraphBuilder::build` refuses it otherwise.
    pub fn state_with_default(mut self, key: Key, kind: Kind, default: Value) -> Self {
        self.state.insert(key.clone(), kind);
        self.defaults.insert(key, default);
        self
    }

    pub fn config(mut self, key: Key, kind: Kind) -> Self {
        self.config.insert(key, kind);
        self
    }

    pub fn build(self) -> Schema {
        Schema {
            state: self.state,
            config: self.config,
            defaults: self.defaults,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_schema_when_round_tripped_then_identical() {
        let schema = Schema::builder()
            .state(Key::new("chat").unwrap(), Kind::Conversation)
            .state(Key::new("items").unwrap(), Kind::list(Kind::Str))
            .config(Key::new("model").unwrap(), Kind::Str)
            .build();
        let json = serde_json::to_string(&schema).unwrap();
        assert_eq!(serde_json::from_str::<Schema>(&json).unwrap(), schema);
    }

    #[test]
    fn given_schema_with_a_default_when_round_tripped_then_identical() {
        let schema = Schema::builder()
            .state_with_default(Key::new("limit").unwrap(), Kind::Int, Value::int(3))
            .build();
        let json = serde_json::to_string(&schema).unwrap();
        assert_eq!(serde_json::from_str::<Schema>(&json).unwrap(), schema);
    }

    #[test]
    fn given_schema_json_without_defaults_when_read_then_no_default() {
        let json = r#"{"state":{"n":{"type":"int"}},"config":{}}"#;
        let schema = serde_json::from_str::<Schema>(json).unwrap();
        assert!(schema.defaults.is_empty());
        assert_eq!(
            schema.default_value(&Key::new("n").unwrap()),
            Some(Value::int(0))
        );
    }

    #[test]
    fn given_default_of_the_wrong_kind_when_checked_then_kind_mismatch() {
        let schema = Schema::builder()
            .state_with_default(Key::new("limit").unwrap(), Kind::Int, Value::str("3"))
            .build();
        assert!(matches!(
            schema.check_defaults(),
            Err(GraphError::KindMismatch { .. })
        ));
    }
}
