use crate::error::GraphError;
use crate::state::{Config, Kind, Schema};
use crate::value::Key;

/// An on/off setting, given as a fixed value or read from a bool
/// configuration key, so the host decides at run time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Switch {
    Fixed(bool),
    Config(Key),
}

impl Switch {
    /// Checks the switch against the schema of a graph: a `Config` key must
    /// be a bool configuration key (`SwitchKeyMismatch`); a fixed value
    /// always passes. A node of the caller that holds a `Switch` calls it
    /// from its own `Node::check`.
    ///
    /// ```
    /// use br_llm_graph::{GraphError, Key, Schema, Switch};
    ///
    /// let deep = Key::new("deep").unwrap();
    /// let schema = Schema::builder().build();
    /// assert!(matches!(
    ///     Switch::Config(deep).check(&schema),
    ///     Err(GraphError::SwitchKeyMismatch { .. })
    /// ));
    /// ```
    pub fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        match self {
            Switch::Fixed(_) => Ok(()),
            Switch::Config(key) => match schema.config.get(key) {
                Some(Kind::Bool) => Ok(()),
                Some(_) | None => Err(GraphError::SwitchKeyMismatch { key: key.clone() }),
            },
        }
    }

    /// The setting for this run: the fixed value, or the value of the
    /// configuration key (`UnknownKey` when `config` does not declare it,
    /// `KindMismatch` when it holds another kind).
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use br_llm_graph::{Config, Key, Kind, Schema, Switch, Value};
    ///
    /// let deep = Key::new("deep").unwrap();
    /// let schema = Schema::builder().config(deep.clone(), Kind::Bool).build();
    /// let config = Config::new(&schema, BTreeMap::from([(deep.clone(), Value::bool(true))])).unwrap();
    /// assert!(Switch::Config(deep).resolve(&config).unwrap());
    /// ```
    pub fn resolve(&self, config: &Config) -> Result<bool, GraphError> {
        match self {
            Switch::Fixed(value) => Ok(*value),
            Switch::Config(key) => config.bool(key),
        }
    }
}

#[cfg(test)]
#[path = "switch_tests.rs"]
mod tests;
