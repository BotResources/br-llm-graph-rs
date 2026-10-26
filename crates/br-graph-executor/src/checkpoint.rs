use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::ser::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use serde_json::value::RawValue;

use br_graph_contract::{Fields, State, Structure};

use crate::ResumeError;
use crate::panic_text::panic_reason;

#[derive(Debug, Clone)]
pub struct Checkpoint {
    state: String,
    structure: Structure,
    nodes: Vec<String>,
}

#[derive(Serialize)]
struct WireOut<'a> {
    state: &'a RawValue,
    structure: &'a Structure,
    nodes: &'a [String],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireValue {
    state: Value,
    structure: Structure,
    nodes: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRaw {
    state: Box<RawValue>,
    structure: Structure,
    nodes: Vec<String>,
}

impl Serialize for Checkpoint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let state = serde_json::from_str::<&RawValue>(&self.state).map_err(S::Error::custom)?;
        WireOut {
            state,
            structure: &self.structure,
            nodes: &self.nodes,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Checkpoint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = WireValue::deserialize(deserializer)?;
        Ok(Self {
            state: wire.state.to_string(),
            structure: wire.structure,
            nodes: wire.nodes,
        })
    }
}

impl PartialEq for Checkpoint {
    fn eq(&self, other: &Self) -> bool {
        self.structure == other.structure
            && self.nodes == other.nodes
            && (self.state == other.state || same_json(&self.state, &other.state))
    }
}

fn same_json(left: &str, right: &str) -> bool {
    match (
        serde_json::from_str::<Value>(left),
        serde_json::from_str::<Value>(right),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
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
        let wire =
            serde_json::from_slice::<WireRaw>(json).map_err(|error| ResumeError::Malformed {
                reason: error.to_string(),
            })?;
        Ok(Checkpoint {
            state: wire.state.get().to_owned(),
            structure: wire.structure,
            nodes: wire.nodes,
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
            state: "null".to_owned(),
            structure: structure.clone(),
            nodes,
        }
    }
}
