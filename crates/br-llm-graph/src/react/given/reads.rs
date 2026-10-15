//! The state and configuration keys a function reads, declared with it:
//! checked against the graph a node runs in, or declared by the graph of an
//! agent.

use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::state::{ConfigKey, Kind, Schema, StateKey, ValueType};
use crate::value::Key;

/// The state and configuration keys a function given to a model-calling node
/// reads, declared with it, each with its kind.
///
/// A key is declared by name, which gives its typed key, or from a typed key
/// the runner already holds. The function's context gives only the keys
/// declared here (`RequestContext::get`, `RequestContext::config`).
///
/// What the keys become depends on the node. In an LLM call, they are keys of
/// the graph the call runs in: the build refuses a key that graph does not
/// declare (`UnknownKey`) or declares with another kind (`KindMismatch`). An
/// agent runs as a called graph: each state key becomes an input of its
/// graph and each configuration key a configuration key of it; a key read by
/// several functions is declared once, and a key the graph already has with
/// another meaning is refused (`KeyCollision`).
///
/// ```
/// use br_llm_graph::{ConfigKey, Reads, StateKey};
///
/// let mut reads = Reads::new();
/// let topic: StateKey<String> = reads.state("topic").unwrap();
/// let tone: ConfigKey<String> = reads.config("tone").unwrap();
/// assert_eq!((topic.as_str(), tone.as_str()), ("topic", "tone"));
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reads {
    state: Vec<(Key, Kind)>,
    config: Vec<(Key, Kind)>,
}

impl Reads {
    /// No key read.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a read of the state key named `name`, of the kind of `T`, and
    /// gives its typed key. A name that is not an identifier is refused
    /// (`Identifier`).
    pub fn state<T: ValueType>(
        &mut self,
        name: impl Into<String>,
    ) -> Result<StateKey<T>, GraphError> {
        let key = StateKey::declared(Key::new(name)?);
        self.read_state(&key);
        Ok(key)
    }

    /// Declares a read of the configuration key named `name`, of the kind of
    /// `T`, and gives its typed key. A name that is not an identifier is
    /// refused (`Identifier`).
    pub fn config<T: ValueType>(
        &mut self,
        name: impl Into<String>,
    ) -> Result<ConfigKey<T>, GraphError> {
        let key = ConfigKey::declared(Key::new(name)?);
        self.read_config(&key);
        Ok(key)
    }

    /// Declares a read of the state key `key`.
    pub fn read_state<T: ValueType>(&mut self, key: &StateKey<T>) {
        self.state.push((key.key().clone(), T::kind()));
    }

    /// Declares a read of the configuration key `key`.
    pub fn read_config<T: ValueType>(&mut self, key: &ConfigKey<T>) {
        self.config.push((key.key().clone(), T::kind()));
    }

    /// The reads of the state key `key` alone.
    pub(crate) fn of_state<T: ValueType>(key: &StateKey<T>) -> Self {
        let mut reads = Self::new();
        reads.read_state(key);
        reads
    }

    /// The reads of the configuration key `key` alone.
    pub(crate) fn of_config<T: ValueType>(key: &ConfigKey<T>) -> Self {
        let mut reads = Self::new();
        reads.read_config(key);
        reads
    }

    /// Whether the state key named `key` is declared.
    pub(crate) fn reads_state(&self, key: &Key) -> bool {
        self.state.iter().any(|(read, _)| read == key)
    }

    /// Whether the configuration key named `key` is declared.
    pub(crate) fn reads_config(&self, key: &Key) -> bool {
        self.config.iter().any(|(read, _)| read == key)
    }

    /// Checks each declared key against the schema of the graph the node runs
    /// in: a key it does not declare is refused (`UnknownKey`), a key it
    /// declares with another kind too (`KindMismatch`, naming the declared
    /// kind and the graph's).
    pub(crate) fn check(&self, schema: &Schema) -> Result<(), GraphError> {
        for (key, kind) in &self.state {
            check_one(&schema.state, key, kind)?;
        }
        for (key, kind) in &self.config {
            check_one(&schema.config, key, kind)?;
        }
        Ok(())
    }
}

fn check_one(declared: &BTreeMap<Key, Kind>, key: &Key, kind: &Kind) -> Result<(), GraphError> {
    match declared.get(key) {
        Some(found) if found == kind => Ok(()),
        Some(found) => Err(GraphError::KindMismatch {
            key: key.clone(),
            expected: kind.clone(),
            found: found.tag(),
        }),
        None => Err(GraphError::UnknownKey { key: key.clone() }),
    }
}

/// Declares the keys `reads` read in the schema of an agent's graph: each
/// state key as an input, each configuration key as a configuration key. A
/// key read by several functions, with one kind, is declared once; a key
/// the schema already has, or one declared again with another kind, is
/// refused (`KeyCollision`). Gives the input keys, in order of declaration.
pub(crate) fn declare_reads<'r>(
    schema: &mut Schema,
    reads: impl IntoIterator<Item = &'r Reads>,
) -> Result<Vec<Key>, GraphError> {
    let mut state: BTreeMap<Key, Kind> = BTreeMap::new();
    let mut config: BTreeMap<Key, Kind> = BTreeMap::new();
    let mut inputs = Vec::new();
    for reads in reads {
        for (key, kind) in &reads.state {
            if declare_once(&mut schema.state, &mut state, key, kind)? {
                inputs.push(key.clone());
            }
        }
        for (key, kind) in &reads.config {
            declare_once(&mut schema.config, &mut config, key, kind)?;
        }
    }
    Ok(inputs)
}

/// Declares `key` in `declared` unless this declaration already did, with
/// the same kind. Whether it declared it now.
fn declare_once(
    declared: &mut BTreeMap<Key, Kind>,
    seen: &mut BTreeMap<Key, Kind>,
    key: &Key,
    kind: &Kind,
) -> Result<bool, GraphError> {
    match seen.get(key) {
        Some(earlier) if earlier == kind => return Ok(false),
        Some(_) => return Err(GraphError::KeyCollision { key: key.clone() }),
        None => {}
    }
    if declared.contains_key(key) {
        return Err(GraphError::KeyCollision { key: key.clone() });
    }
    declared.insert(key.clone(), kind.clone());
    seen.insert(key.clone(), kind.clone());
    Ok(true)
}

#[cfg(test)]
#[path = "reads_tests.rs"]
mod tests;
