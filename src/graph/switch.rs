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
    pub(crate) fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        match self {
            Switch::Fixed(_) => Ok(()),
            Switch::Config(key) => match schema.config.get(key) {
                Some(Kind::Bool) => Ok(()),
                Some(_) | None => Err(GraphError::SwitchKeyMismatch { key: key.clone() }),
            },
        }
    }

    pub(crate) fn resolve(&self, config: &Config) -> Result<bool, GraphError> {
        match self {
            Switch::Fixed(value) => Ok(*value),
            Switch::Config(key) => config.bool(key),
        }
    }
}

#[cfg(test)]
#[path = "switch_tests.rs"]
mod tests;
