use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use br_graph_contract::{Fields, State, Structure};

use crate::ResumeError;
use crate::panic_text::panic_reason;

#[derive(Debug, Clone, PartialEq)]
pub struct Checkpoint {
    state: String,
    structure: Structure,
    nodes: Vec<String>,
}

#[derive(Serialize)]
struct WireOut<'a> {
    state: &'a str,
    structure: &'a Structure,
    nodes: &'a [String],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireIn {
    state: String,
    structure: Structure,
    nodes: Vec<String>,
}

impl Serialize for Checkpoint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WireOut {
            state: &self.state,
            structure: &self.structure,
            nodes: &self.nodes,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Checkpoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Checkpoint::from(WireIn::deserialize(deserializer)?))
    }
}

impl From<WireIn> for Checkpoint {
    fn from(wire: WireIn) -> Self {
        Self {
            state: wire.state,
            structure: wire.structure,
            nodes: wire.nodes,
        }
    }
}

impl Checkpoint {
    pub fn state_json(&self) -> &str {
        &self.state
    }

    pub fn structure(&self) -> &Structure {
        &self.structure
    }

    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }

    pub fn from_json_slice(json: &[u8]) -> Result<Checkpoint, ResumeError> {
        serde_json::from_slice::<WireIn>(json)
            .map(Checkpoint::from)
            .map_err(|error| ResumeError::Malformed {
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
        let state = String::from_utf8(bytes).map_err(|error| error.to_string())?;
        Ok(Checkpoint {
            state,
            structure: structure.clone(),
            nodes,
        })
    }

    pub(crate) fn unreadable(structure: &Structure, nodes: Vec<String>) -> Checkpoint {
        Checkpoint {
            state: String::new(),
            structure: structure.clone(),
            nodes,
        }
    }
}
