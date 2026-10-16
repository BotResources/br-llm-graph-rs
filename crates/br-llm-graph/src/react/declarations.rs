//! The keys an agent's graph declares for what its caller and its runner
//! give: the state keys its functions read, its enabled key and its extra
//! inputs become inputs, the configuration keys its functions read become
//! configuration keys. One key declared twice with one kind, and so one
//! meaning, is declared once; a key of another kind, or a key the graph
//! keeps for itself, collides.

use crate::error::GraphError;
use crate::react::given::Reads;
use crate::state::{Kind, Schema};
use crate::value::Key;

/// The schema of an agent's graph under construction, and the inputs it
/// takes from its caller.
pub(crate) struct Declarations {
    schema: Schema,
    /// The state keys the graph keeps for itself (`opening`, `answer`, its
    /// private keys, the round limit's flag...): nothing given may name one.
    reserved: Vec<Key>,
    inputs: Vec<Key>,
}

impl Declarations {
    /// The declarations of a graph whose own keys `schema` already holds,
    /// the state keys among them that nothing given may name in `reserved`.
    pub(crate) fn new(schema: Schema, reserved: Vec<Key>) -> Self {
        Self {
            schema,
            reserved,
            inputs: Vec::new(),
        }
    }

    /// Declares the input `key`, of kind `kind`. A key already declared with
    /// that kind (a key a function reads, the enabled key, an extra input) is
    /// the same input, declared once; a key the graph keeps for itself, or
    /// one declared with another kind, is refused (`KeyCollision`).
    pub(crate) fn input(&mut self, key: &Key, kind: Kind) -> Result<(), GraphError> {
        if self.reserved.contains(key) {
            return Err(GraphError::KeyCollision { key: key.clone() });
        }
        match self.schema.state.get(key) {
            Some(found) if *found == kind => Ok(()),
            Some(_) => Err(GraphError::KeyCollision { key: key.clone() }),
            None => {
                self.schema.state.insert(key.clone(), kind);
                self.inputs.push(key.clone());
                Ok(())
            }
        }
    }

    /// Declares the configuration key `key`, of kind `kind`. A key already
    /// declared with that kind (a key a function reads, the key of a limit
    /// or a count) is declared once; one declared with another kind is
    /// refused (`KeyCollision`).
    pub(crate) fn config(&mut self, key: &Key, kind: Kind) -> Result<(), GraphError> {
        match self.schema.config.get(key) {
            Some(found) if *found == kind => Ok(()),
            Some(_) => Err(GraphError::KeyCollision { key: key.clone() }),
            None => {
                self.schema.config.insert(key.clone(), kind);
                Ok(())
            }
        }
    }

    /// Declares the keys `reads` read: each state key as an input, each
    /// configuration key as a configuration key.
    pub(crate) fn reads(&mut self, reads: &Reads) -> Result<(), GraphError> {
        for (key, kind) in reads.state_reads() {
            self.input(key, kind.clone())?;
        }
        for (key, kind) in reads.config_reads() {
            self.config(key, kind.clone())?;
        }
        Ok(())
    }

    /// The schema as declared, and the inputs, in order of declaration.
    pub(crate) fn finish(self) -> (Schema, Vec<Key>) {
        (self.schema, self.inputs)
    }
}

#[cfg(test)]
#[path = "declarations_tests.rs"]
mod tests;
