use std::collections::BTreeMap;

use crate::state::kind::Kind;
use crate::value::Key;

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Schema {
    pub state: BTreeMap<Key, Kind>,
    pub config: BTreeMap<Key, Kind>,
}

impl Schema {
    pub fn new(state: BTreeMap<Key, Kind>, config: BTreeMap<Key, Kind>) -> Self {
        Self { state, config }
    }

    pub fn builder() -> SchemaBuilder {
        SchemaBuilder::default()
    }
}

#[derive(Default)]
pub struct SchemaBuilder {
    state: BTreeMap<Key, Kind>,
    config: BTreeMap<Key, Kind>,
}

impl SchemaBuilder {
    pub fn state(mut self, key: Key, kind: Kind) -> Self {
        self.state.insert(key, kind);
        self
    }

    pub fn config(mut self, key: Key, kind: Kind) -> Self {
        self.config.insert(key, kind);
        self
    }

    pub fn build(self) -> Schema {
        Schema {
            state: self.state,
            config: self.config,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_schema_when_round_tripped_then_identical() {
        let schema = Schema::builder()
            .state(Key::new("chat").unwrap(), Kind::Conversation)
            .state(Key::new("items").unwrap(), Kind::list(Kind::Str))
            .config(Key::new("model").unwrap(), Kind::Str)
            .build();
        let json = serde_json::to_string(&schema).unwrap();
        assert_eq!(serde_json::from_str::<Schema>(&json).unwrap(), schema);
    }
}
