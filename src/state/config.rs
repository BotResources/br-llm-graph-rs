use std::collections::BTreeMap;

use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::schema::Schema;
use crate::state::state::SCHEMA_VERSION;
use crate::state::value::Value;
use crate::value::Key;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Config {
    schema_version: &'static str,
    schema: BTreeMap<Key, Kind>,
    values: BTreeMap<Key, Value>,
}

#[derive(serde::Deserialize)]
struct RawConfig {
    schema_version: String,
    schema: BTreeMap<Key, Kind>,
    values: BTreeMap<Key, Value>,
}

impl TryFrom<RawConfig> for Config {
    type Error = GraphError;

    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        if raw.schema_version != SCHEMA_VERSION {
            return Err(GraphError::SchemaMismatch {
                found: raw.schema_version,
            });
        }
        Config::assemble(raw.schema, raw.values)
    }
}

impl<'de> serde::Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawConfig::deserialize(deserializer)?;
        Config::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl Config {
    pub fn new(schema: &Schema, values: BTreeMap<Key, Value>) -> Result<Self, GraphError> {
        Config::assemble(schema.config.clone(), values)
    }

    fn assemble(
        schema: BTreeMap<Key, Kind>,
        values: BTreeMap<Key, Value>,
    ) -> Result<Self, GraphError> {
        for (key, value) in &values {
            match schema.get(key) {
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
        for key in schema.keys() {
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

    pub(crate) fn get(&self, key: &Key) -> Result<&Value, GraphError> {
        self.values
            .get(key)
            .ok_or_else(|| GraphError::MissingKey { key: key.clone() })
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
            other => match self.schema.get(key) {
                Some(expected) => Err(self.mismatch(key, expected.clone(), other)),
                None => Err(GraphError::UnknownKey { key: key.clone() }),
            },
        }
    }

    pub fn conversation(&self, key: &Key) -> Result<&Conversation, GraphError> {
        match self.get(key)? {
            Value::Conversation(conversation) => Ok(conversation),
            other => Err(self.mismatch(key, Kind::Conversation, other)),
        }
    }

    fn mismatch(&self, key: &Key, expected: Kind, found: &Value) -> GraphError {
        GraphError::KindMismatch {
            key: key.clone(),
            expected,
            found: found.tag(),
        }
    }
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
