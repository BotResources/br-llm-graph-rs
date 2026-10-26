use std::collections::BTreeMap;

use br_graph_contract::{Fields, Reason, State};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Kv {
    pub values: BTreeMap<String, i64>,
    pub pad: String,
    #[serde(default)]
    pub wide: u128,
}

#[derive(Debug)]
pub enum Change {
    Set(String, i64),
    Add(String, i64),
    Empty,
    Refuse,
    Invalid,
    Grow(usize),
    Wide(u128),
    Boom,
}

const INVALID_KEY: &str = "invalid";
const VALIDATION_PANIC_KEY: &str = "panic_on_validate";

impl Fields for Kv {
    type Update = Change;

    fn empty_update() -> Change {
        Change::Empty
    }

    fn validate(&self) -> Result<(), Reason> {
        if self.values.contains_key(VALIDATION_PANIC_KEY) {
            panic!("validation panics");
        }
        if self.values.contains_key(INVALID_KEY) {
            return Err(Reason::new("the fields hold the invalid key"));
        }
        Ok(())
    }

    fn apply(&mut self, update: Change) -> Result<(), Reason> {
        match update {
            Change::Set(key, value) => {
                self.values.insert(key, value);
            }
            Change::Add(key, amount) => {
                *self.values.entry(key).or_insert(0) += amount;
            }
            Change::Empty => {}
            Change::Refuse => return Err(Reason::new("the update is refused")),
            Change::Invalid => {
                self.values.insert(INVALID_KEY.to_owned(), 1);
            }
            Change::Grow(bytes) => self.pad.push_str(&"x".repeat(bytes)),
            Change::Wide(value) => self.wide = value,
            Change::Boom => panic!("the fields panic"),
        }
        Ok(())
    }
}

pub fn set(key: &str, value: i64) -> Change {
    Change::Set(key.to_owned(), value)
}

pub fn add(key: &str, amount: i64) -> Change {
    Change::Add(key.to_owned(), amount)
}

pub fn kv(pairs: &[(&str, i64)]) -> Kv {
    Kv {
        values: pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), *value))
            .collect(),
        pad: String::new(),
        wide: 0,
    }
}

pub fn state(pairs: &[(&str, i64)]) -> State<Kv> {
    State::create(kv(pairs)).unwrap()
}

pub fn value_of(fields: &Kv, key: &str) -> Option<i64> {
    fields.values.get(key).copied()
}
