use std::collections::BTreeMap;

use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::schema::Schema;
use crate::state::typed_key::StateKey;
use crate::state::value::Value;
use crate::state::value_type::{ValueType, read_as};
use crate::value::Key;

pub const SCHEMA_VERSION: &str = "br-llm-graph/1";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct State {
    schema_version: &'static str,
    schema: Schema,
    values: BTreeMap<Key, Value>,
}

#[derive(serde::Deserialize)]
struct RawState {
    schema_version: String,
    schema: Schema,
    values: BTreeMap<Key, Value>,
}

impl TryFrom<RawState> for State {
    type Error = GraphError;

    fn try_from(raw: RawState) -> Result<Self, Self::Error> {
        if raw.schema_version != SCHEMA_VERSION {
            return Err(GraphError::SchemaMismatch {
                found: raw.schema_version,
            });
        }
        State::new(raw.schema, raw.values)
    }
}

impl<'de> serde::Deserialize<'de> for State {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawState::deserialize(deserializer)?;
        State::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl State {
    pub fn new(schema: Schema, values: BTreeMap<Key, Value>) -> Result<Self, GraphError> {
        for (key, value) in &values {
            match schema.state.get(key) {
                None => {
                    return Err(GraphError::UnknownKey { key: key.clone() });
                }
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
        for key in schema.state.keys() {
            if !values.contains_key(key) {
                return Err(GraphError::MissingKey { key: key.clone() });
            }
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            schema,
            values,
        })
    }

    pub(crate) fn empty() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            schema: Schema::default(),
            values: BTreeMap::new(),
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }

    pub(crate) fn kind_of(&self, key: &Key) -> Option<&Kind> {
        self.schema.state.get(key)
    }

    /// The value of `key`: `UnknownKey` when the schema does not declare it,
    /// `MissingKey` when it is declared but holds no value.
    pub(crate) fn value(&self, key: &Key) -> Result<&Value, GraphError> {
        if !self.schema.state.contains_key(key) {
            return Err(GraphError::UnknownKey { key: key.clone() });
        }
        self.values
            .get(key)
            .ok_or_else(|| GraphError::MissingKey { key: key.clone() })
    }

    pub(crate) fn values_mut(&mut self) -> &mut BTreeMap<Key, Value> {
        &mut self.values
    }

    /// The value of `key`, as its type: an `i64`, an `f64`, a `&str`, a
    /// `bool`, a `Vec` of the elements' reads, or a `&Conversation`.
    ///
    /// The state must declare the key with its kind: a key it does not declare
    /// is refused (`UnknownKey`), a key declared with another kind too
    /// (`KindMismatch`), as for a key of another schema.
    pub fn get<T: ValueType>(&self, key: &StateKey<T>) -> Result<T::Read<'_>, GraphError> {
        let value = self.value(key.key())?;
        let declared = self
            .kind_of(key.key())
            .ok_or_else(|| GraphError::UnknownKey {
                key: key.key().clone(),
            })?;
        read_as::<T>(key.key(), declared, value)
    }

    /// The int value of the key named `key`, read by name.
    pub fn int(&self, key: &Key) -> Result<i64, GraphError> {
        match self.value(key)? {
            Value::Int(v) => Ok(*v),
            other => Err(self.mismatch(key, Kind::Int, other)),
        }
    }

    /// The float value of the key named `key`, read by name.
    pub fn float(&self, key: &Key) -> Result<f64, GraphError> {
        match self.value(key)? {
            Value::Float(v) => Ok(v.get()),
            other => Err(self.mismatch(key, Kind::Float, other)),
        }
    }

    /// The string value of the key named `key`, read by name.
    pub fn str(&self, key: &Key) -> Result<&str, GraphError> {
        match self.value(key)? {
            Value::Str(v) => Ok(v),
            other => Err(self.mismatch(key, Kind::Str, other)),
        }
    }

    /// The bool value of the key named `key`, read by name.
    pub fn bool(&self, key: &Key) -> Result<bool, GraphError> {
        match self.value(key)? {
            Value::Bool(v) => Ok(*v),
            other => Err(self.mismatch(key, Kind::Bool, other)),
        }
    }

    /// The items of the list key named `key`, read by name. A list read
    /// names no element kind: a key that holds another kind gives
    /// `KindMismatch` whose expected kind is a list (of the key's own kind)
    /// and whose found tag is the value's.
    pub fn list(&self, key: &Key) -> Result<&[Value], GraphError> {
        match self.value(key)? {
            Value::List(items) => Ok(items),
            other => match self.kind_of(key) {
                Some(declared) => Err(self.mismatch(key, Kind::list(declared.clone()), other)),
                None => Err(GraphError::UnknownKey { key: key.clone() }),
            },
        }
    }

    /// The conversation value of the key named `key`, read by name.
    pub fn conversation(&self, key: &Key) -> Result<&Conversation, GraphError> {
        match self.value(key)? {
            Value::Conversation(conversation) => Ok(conversation),
            other => Err(self.mismatch(key, Kind::Conversation, other)),
        }
    }

    pub fn derive(&self, key: &Key, value: Value) -> Result<State, GraphError> {
        let mut derived = self.clone();
        let kind = derived
            .kind_of(key)
            .ok_or_else(|| GraphError::UnknownKey { key: key.clone() })?;
        if !value.matches(kind) {
            return Err(GraphError::KindMismatch {
                key: key.clone(),
                expected: kind.clone(),
                found: value.tag(),
            });
        }
        derived.values.insert(key.clone(), value);
        Ok(derived)
    }

    fn mismatch(&self, key: &Key, expected: Kind, found: &Value) -> GraphError {
        GraphError::KindMismatch {
            key: key.clone(),
            expected,
            found: found.tag(),
        }
    }
}

impl std::fmt::Display for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "state ({} keys)", self.values.len())?;
        for (key, value) in &self.values {
            writeln!(f, "  {key} = {value}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
