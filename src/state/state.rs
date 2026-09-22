use std::collections::BTreeMap;

use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::schema::Schema;
use crate::state::value::Value;
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

    pub(crate) fn get(&self, key: &Key) -> Result<&Value, GraphError> {
        self.values
            .get(key)
            .ok_or_else(|| GraphError::MissingKey { key: key.clone() })
    }

    pub(crate) fn values_mut(&mut self) -> &mut BTreeMap<Key, Value> {
        &mut self.values
    }

    pub(crate) fn value(&self, key: &Key) -> Result<&Value, GraphError> {
        self.get(key)
    }

    pub fn int(&self, key: &Key) -> Result<i64, GraphError> {
        match self.get(key)? {
            Value::Int(v) => Ok(*v),
            other => Err(self.mismatch(key, Kind::Int, other)),
        }
    }

    pub fn float(&self, key: &Key) -> Result<f64, GraphError> {
        match self.get(key)? {
            Value::Float(v) => Ok(v.get()),
            other => Err(self.mismatch(key, Kind::Float, other)),
        }
    }

    pub fn str(&self, key: &Key) -> Result<&str, GraphError> {
        match self.get(key)? {
            Value::Str(v) => Ok(v),
            other => Err(self.mismatch(key, Kind::Str, other)),
        }
    }

    pub fn bool(&self, key: &Key) -> Result<bool, GraphError> {
        match self.get(key)? {
            Value::Bool(v) => Ok(*v),
            other => Err(self.mismatch(key, Kind::Bool, other)),
        }
    }

    pub fn list(&self, key: &Key) -> Result<&[Value], GraphError> {
        match self.get(key)? {
            Value::List(items) => Ok(items),
            other => Err(self.mismatch(key, self.declared_kind(key), other)),
        }
    }

    fn declared_kind(&self, key: &Key) -> Kind {
        match self.kind_of(key) {
            Some(kind) => kind.clone(),
            None => Kind::list(Kind::Str),
        }
    }

    pub fn conversation(&self, key: &Key) -> Result<&Conversation, GraphError> {
        match self.get(key)? {
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
