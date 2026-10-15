use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::typed_key::{ConfigKey, StateKey};
use crate::state::value::Value;
use crate::state::value_type::ValueType;
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
    /// Declares the state key named `name`, of the kind of `T`, and gives its
    /// typed key. Its start value is the neutral value of its kind. A name
    /// that is not an identifier is refused (`Identifier`).
    ///
    /// ```
    /// use br_llm_graph::{Schema, StateKey};
    /// use br_llm_messages::Conversation;
    ///
    /// let mut schema = Schema::builder();
    /// let attempts: StateKey<i64> = schema.state("attempts").unwrap();
    /// let notes = schema.state::<Vec<String>>("notes").unwrap();
    /// let chat = schema.state::<Conversation>("chat").unwrap();
    /// let schema = schema.build();
    /// assert_eq!(schema.state.len(), 3);
    /// ```
    ///
    /// Only the Rust types of the kinds can be declared (`ValueType`):
    ///
    /// ```compile_fail
    /// let mut schema = br_llm_graph::Schema::builder();
    /// let attempts = schema.state::<u32>("attempts");
    /// ```
    ///
    /// Declaring a name again replaces its kind, as `state_by_name` does: a
    /// typed key given before for that name then reads as `KindMismatch`.
    pub fn state<T: ValueType>(
        &mut self,
        name: impl Into<String>,
    ) -> Result<StateKey<T>, GraphError> {
        let key = Key::new(name)?;
        self.state.insert(key.clone(), T::kind());
        Ok(StateKey::declared(key))
    }

    /// Declares the state key named `name`, of the kind of `T`, with the start
    /// value `default`, and gives its typed key. A name that is not an
    /// identifier is refused (`Identifier`), a float default that is not
    /// finite too (`FloatNotFinite`).
    ///
    /// ```
    /// use br_llm_graph::{Schema, StateKey, Value};
    ///
    /// let mut schema = Schema::builder();
    /// let tone: StateKey<String> = schema.state_with_default("tone", "plain".to_owned()).unwrap();
    /// let schema = schema.build();
    /// assert_eq!(schema.default_value(tone.key()), Some(Value::str("plain")));
    /// ```
    pub fn state_with_default<T: ValueType>(
        &mut self,
        name: impl Into<String>,
        default: T,
    ) -> Result<StateKey<T>, GraphError> {
        let key = Key::new(name)?;
        let default = default.into_value()?;
        self.state.insert(key.clone(), T::kind());
        self.defaults.insert(key.clone(), default);
        Ok(StateKey::declared(key))
    }

    /// Declares the configuration key named `name`, of the kind of `T`, and
    /// gives its typed key. A name that is not an identifier is refused
    /// (`Identifier`). A state key and a configuration key may share a name.
    pub fn config<T: ValueType>(
        &mut self,
        name: impl Into<String>,
    ) -> Result<ConfigKey<T>, GraphError> {
        let key = Key::new(name)?;
        self.config.insert(key.clone(), T::kind());
        Ok(ConfigKey::declared(key))
    }

    /// Declares the state key `key` of kind `kind`, known only at run time (a
    /// graph described as data). A key declared again takes the last kind.
    pub fn state_by_name(mut self, key: Key, kind: Kind) -> Self {
        self.state.insert(key, kind);
        self
    }

    /// Declares a state key with an explicit start value, by name. The value
    /// must have the kind of the key; `GraphBuilder::build` refuses it
    /// otherwise (`KindMismatch`).
    pub fn state_by_name_with_default(mut self, key: Key, kind: Kind, default: Value) -> Self {
        self.state.insert(key.clone(), kind);
        self.defaults.insert(key, default);
        self
    }

    /// Declares the configuration key `key` of kind `kind`, by name.
    pub fn config_by_name(mut self, key: Key, kind: Kind) -> Self {
        self.config.insert(key, kind);
        self
    }

    /// The schema of every key declared so far.
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
            .state_by_name(Key::new("chat").unwrap(), Kind::Conversation)
            .state_by_name(Key::new("items").unwrap(), Kind::list(Kind::Str))
            .config_by_name(Key::new("model").unwrap(), Kind::Str)
            .build();
        let json = serde_json::to_string(&schema).unwrap();
        assert_eq!(serde_json::from_str::<Schema>(&json).unwrap(), schema);
    }

    #[test]
    fn given_schema_with_a_default_when_round_tripped_then_identical() {
        let schema = Schema::builder()
            .state_by_name_with_default(Key::new("limit").unwrap(), Kind::Int, Value::int(3))
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
            .state_by_name_with_default(Key::new("limit").unwrap(), Kind::Int, Value::str("3"))
            .build();
        assert!(matches!(
            schema.check_defaults(),
            Err(GraphError::KindMismatch { .. })
        ));
    }
}
