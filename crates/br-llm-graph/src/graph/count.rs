use crate::error::GraphError;
use crate::state::{Config, Kind, Schema};
use crate::value::Key;

/// A bound that may be zero, given as a fixed number or read from an int
/// configuration key, so the host decides at run time. `Limit` is its
/// positive counterpart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Count {
    Fixed(usize),
    Config(Key),
}

impl Count {
    /// Checks the count against the schema of a graph: a `Config` key must be
    /// an int configuration key (`CountKeyMismatch`); a fixed number always
    /// passes. A node of the caller that holds a `Count` calls it from its own
    /// `Node::check`.
    ///
    /// ```
    /// use br_llm_graph::{Count, Key, Kind, Schema};
    ///
    /// let budget = Key::new("budget").unwrap();
    /// let schema = Schema::builder().config(budget.clone(), Kind::Int).build();
    /// assert!(Count::Config(budget).check(&schema).is_ok());
    /// ```
    pub fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        match self {
            Count::Fixed(_) => Ok(()),
            Count::Config(key) => match schema.config.get(key) {
                Some(Kind::Int) => Ok(()),
                Some(_) | None => Err(GraphError::CountKeyMismatch { key: key.clone() }),
            },
        }
    }

    /// The count for this run: the fixed number, or the value of the
    /// configuration key. A negative configuration value is refused
    /// (`CountNegative`), a key `config` does not declare too (`UnknownKey`,
    /// or `KindMismatch` when it holds another kind).
    ///
    /// A node of the caller reads the same configuration as the graph:
    ///
    /// ```
    /// use std::collections::BTreeMap;
    /// use br_llm_graph::{Config, Count, Key, Kind, Schema, Value};
    ///
    /// let budget = Key::new("budget").unwrap();
    /// let schema = Schema::builder().config(budget.clone(), Kind::Int).build();
    /// let config = Config::new(&schema, BTreeMap::from([(budget.clone(), Value::int(0))])).unwrap();
    /// assert_eq!(Count::Config(budget).resolve(&config).unwrap(), 0);
    /// ```
    pub fn resolve(&self, config: &Config) -> Result<usize, GraphError> {
        match self {
            Count::Fixed(value) => Ok(*value),
            Count::Config(key) => {
                let value = config.int(key)?;
                usize::try_from(value).map_err(|_| GraphError::CountNegative {
                    key: key.clone(),
                    value,
                })
            }
        }
    }
}

#[cfg(test)]
#[path = "count_tests.rs"]
mod tests;
