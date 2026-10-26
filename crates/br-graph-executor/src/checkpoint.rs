use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use br_graph_contract::{Fields, State, Structure};

use crate::ResumeError;
use crate::panic_text::panic_reason;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    state: Box<RawValue>,
    structure: Structure,
    nodes: Vec<String>,
}

impl PartialEq for Checkpoint {
    fn eq(&self, other: &Self) -> bool {
        self.state.get() == other.state.get()
            && self.structure == other.structure
            && self.nodes == other.nodes
    }
}

impl Checkpoint {
    pub fn state_json(&self) -> &str {
        self.state.get()
    }

    pub fn structure(&self) -> &Structure {
        &self.structure
    }

    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }

    pub fn from_json_slice(json: &[u8]) -> Result<Checkpoint, ResumeError> {
        serde_json::from_slice(json).map_err(|error| ResumeError::Malformed {
            reason: error.to_string(),
        })
    }

    pub(crate) fn build<F: Fields>(
        state: &State<F>,
        structure: &Structure,
        nodes: Vec<String>,
    ) -> Result<Checkpoint, String> {
        let serialized = catch_unwind(AssertUnwindSafe(|| state.to_json()))
            .map_err(|payload| panic_reason(&*payload))?;
        let bytes = serialized.map_err(|error| error.to_string())?;
        let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
        let state = RawValue::from_string(text).map_err(|error| error.to_string())?;
        Ok(Checkpoint {
            state,
            structure: structure.clone(),
            nodes,
        })
    }

    pub(crate) fn unreadable(structure: &Structure, nodes: Vec<String>) -> Checkpoint {
        Checkpoint {
            state: RawValue::NULL.to_owned(),
            structure: structure.clone(),
            nodes,
        }
    }
}
