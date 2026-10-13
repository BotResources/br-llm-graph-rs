use std::collections::BTreeMap;

use crate::error::GraphError;
use crate::state::{Kind, Schema};
use crate::value::Key;

/// The state keys a graph takes as inputs and gives back as outputs, with
/// their kinds. A key may be both.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Signature {
    pub inputs: BTreeMap<Key, Kind>,
    pub outputs: BTreeMap<Key, Kind>,
}

impl Signature {
    pub(crate) fn resolve(
        schema: &Schema,
        inputs: Vec<Key>,
        outputs: Vec<Key>,
    ) -> Result<Self, GraphError> {
        let mut signature = Signature::default();
        for key in inputs {
            let kind = kind_of(schema, &key)?;
            if signature.inputs.contains_key(&key) {
                return Err(GraphError::DuplicateInput { key });
            }
            signature.inputs.insert(key, kind);
        }
        for key in outputs {
            let kind = kind_of(schema, &key)?;
            if signature.outputs.contains_key(&key) {
                return Err(GraphError::DuplicateOutput { key });
            }
            signature.outputs.insert(key, kind);
        }
        Ok(signature)
    }
}

fn kind_of(schema: &Schema, key: &Key) -> Result<Kind, GraphError> {
    schema
        .state
        .get(key)
        .cloned()
        .ok_or_else(|| GraphError::UnknownKey { key: key.clone() })
}

#[cfg(test)]
#[path = "signature_tests.rs"]
mod tests;
