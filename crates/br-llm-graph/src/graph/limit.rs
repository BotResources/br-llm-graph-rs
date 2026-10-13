use std::num::NonZeroUsize;

use crate::error::GraphError;
use crate::state::{Config, Kind, Schema};
use crate::value::Key;

/// A positive bound, given as a fixed number or read from an int
/// configuration key, so the host decides at run time. `Count` is its
/// counterpart that may be zero.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Limit {
    Fixed(NonZeroUsize),
    Config(Key),
}

impl Limit {
    pub(crate) fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        match self {
            Limit::Fixed(_) => Ok(()),
            Limit::Config(key) => match schema.config.get(key) {
                Some(Kind::Int) => Ok(()),
                Some(_) | None => Err(GraphError::LimitKeyMismatch { key: key.clone() }),
            },
        }
    }

    /// The bound for this run: the fixed number, or the value of the
    /// configuration key. A configuration value below one is refused
    /// (`LimitNotPositive`), a key `config` does not declare too
    /// (`UnknownKey`, or `KindMismatch` when it holds another kind).
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use br_llm_graph::{Config, Key, Kind, Limit, Schema, Value};
    ///
    /// let width = Key::new("width").unwrap();
    /// let schema = Schema::builder().config(width.clone(), Kind::Int).build();
    /// let config = Config::new(&schema, BTreeMap::from([(width.clone(), Value::int(4))])).unwrap();
    /// assert_eq!(Limit::Config(width).resolve(&config).unwrap().get(), 4);
    /// ```
    pub fn resolve(&self, config: &Config) -> Result<NonZeroUsize, GraphError> {
        match self {
            Limit::Fixed(value) => Ok(*value),
            Limit::Config(key) => {
                let value = config.int(key)?;
                usize::try_from(value)
                    .ok()
                    .and_then(NonZeroUsize::new)
                    .ok_or_else(|| GraphError::LimitNotPositive {
                        key: key.clone(),
                        value,
                    })
            }
        }
    }
}

#[cfg(test)]
#[path = "limit_tests.rs"]
mod tests;
