use std::sync::Arc;

use crate::error::GraphError;
use crate::state::{Config, Kind, Schema, State, Value};
use crate::value::Key;

/// A text computed from the state and the configuration: pure and
/// synchronous.
pub type SourceFn = Arc<dyn Fn(&State, &Config) -> Result<String, GraphError> + Send + Sync>;

/// Where a text comes from.
#[derive(Clone)]
pub enum Source {
    /// A string, or a list of strings, read from a configuration key.
    Config(Key),
    /// A string, or a list of strings, read from a state key.
    State(Key),
    /// A text fixed at build.
    Const(String),
    /// A text computed at each model call from the state and the
    /// configuration. The function is pure and synchronous: no I/O, anything
    /// fetched belongs in a node that writes it to the state. It reads only
    /// keys the graph declares otherwise.
    Fn(SourceFn),
}

impl Source {
    /// The text this source gives on this state and configuration. A list of
    /// strings gives its items separated by a blank line.
    pub fn resolve(&self, state: &State, config: &Config) -> Result<String, GraphError> {
        Ok(self.texts(state, config)?.join("\n\n"))
    }

    /// A `State` key must be a declared state key, a `Config` key a declared
    /// configuration key, holding a string or a list of strings
    /// (`SourceKeyMismatch`).
    pub(crate) fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        let (key, kind) = match self {
            Source::State(key) => (key, schema.state.get(key)),
            Source::Config(key) => (key, schema.config.get(key)),
            Source::Const(_) | Source::Fn(_) => return Ok(()),
        };
        match kind {
            Some(Kind::Str) => Ok(()),
            Some(Kind::List { element }) if **element == Kind::Str => Ok(()),
            Some(_) | None => Err(GraphError::SourceKeyMismatch { key: key.clone() }),
        }
    }

    /// The texts this source gives: one, or one per item of a list.
    pub(crate) fn texts(&self, state: &State, config: &Config) -> Result<Vec<String>, GraphError> {
        let mut parts = Vec::new();
        match self {
            Source::Config(key) => append_strings(key, config.value(key)?, &mut parts)?,
            Source::State(key) => append_strings(key, state.value(key)?, &mut parts)?,
            Source::Const(text) => parts.push(text.clone()),
            Source::Fn(text) => parts.push(text(state, config)?),
        }
        Ok(parts)
    }
}

fn append_strings(key: &Key, value: &Value, parts: &mut Vec<String>) -> Result<(), GraphError> {
    match value {
        Value::Str(text) => {
            parts.push(text.clone());
            Ok(())
        }
        Value::List(items) => {
            for item in items {
                match item {
                    Value::Str(text) => parts.push(text.clone()),
                    other => {
                        return Err(GraphError::KindMismatch {
                            key: key.clone(),
                            expected: Kind::Str,
                            found: other.tag(),
                        });
                    }
                }
            }
            Ok(())
        }
        other => Err(GraphError::KindMismatch {
            key: key.clone(),
            expected: Kind::Str,
            found: other.tag(),
        }),
    }
}
