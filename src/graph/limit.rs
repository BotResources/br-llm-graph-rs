use std::num::NonZeroUsize;

use crate::error::GraphError;
use crate::state::{Config, Kind, Schema};
use crate::value::Key;

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

    pub(crate) fn resolve(&self, config: &Config) -> Result<NonZeroUsize, GraphError> {
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
