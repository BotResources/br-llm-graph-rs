use std::fmt;

use br_graph_contract::{CancelHandle, Fields, Reason, State};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub a: bool,
    pub b: bool,
    pub merged: bool,
    pub items: Vec<u32>,
    pub sum: Option<u32>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReportUpdate {
    Nothing,
    Reduced { sum: u32, errors: Vec<String> },
}

impl Report {
    pub fn blank() -> Self {
        Self {
            a: false,
            b: false,
            merged: false,
            items: Vec::new(),
            sum: None,
            errors: Vec::new(),
        }
    }

    pub fn with_items(items: &[u32]) -> Self {
        Self {
            items: items.to_vec(),
            ..Self::blank()
        }
    }

    pub fn filled(a: bool, b: bool, merged: bool) -> Self {
        Self {
            a,
            b,
            merged,
            ..Self::blank()
        }
    }

    pub fn state(self) -> State<Report> {
        State::create(self).expect("a valid report")
    }
}

impl Fields for Report {
    type Update = ReportUpdate;

    fn empty_update() -> ReportUpdate {
        ReportUpdate::Nothing
    }

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, update: ReportUpdate) -> Result<(), Reason> {
        if let ReportUpdate::Reduced { sum, errors } = update {
            self.sum = Some(sum);
            self.errors = errors;
        }
        Ok(())
    }
}

pub fn both_filled(state: &State<Report>) -> bool {
    state.fields().a && state.fields().b
}

pub fn merged(state: &State<Report>) -> bool {
    state.fields().merged
}

pub const BAD_VALUE: u32 = 666;
pub const EXPLODING_VALUE: u32 = 777;
pub const VALIDATE_PANIC: &str = "validate exploded";
pub const APPLY_PANIC: &str = "apply exploded";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub value: u32,
    pub out: Option<u32>,
    pub error: Option<String>,
    pub label: String,
}

#[derive(Clone)]
pub struct Hook(pub CancelHandle);

impl fmt::Debug for Hook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Hook")
    }
}

impl PartialEq for Hook {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemUpdate {
    Nothing,
    Set(u32),
    Record(String),
    Reject,
    Invalidate,
    Trigger(Hook),
    Explode,
}

impl Item {
    pub fn of(value: u32) -> Self {
        Self {
            value,
            out: None,
            error: None,
            label: String::new(),
        }
    }

    pub fn labelled(value: u32, label: &str) -> Self {
        Self {
            label: label.to_owned(),
            ..Self::of(value)
        }
    }
}

impl Fields for Item {
    type Update = ItemUpdate;

    fn empty_update() -> ItemUpdate {
        ItemUpdate::Nothing
    }

    fn validate(&self) -> Result<(), Reason> {
        if self.value == EXPLODING_VALUE {
            panic!("{VALIDATE_PANIC}");
        }
        if self.value == BAD_VALUE || self.out == Some(u32::MAX) {
            return Err(Reason::new("the item is not valid"));
        }
        Ok(())
    }

    fn apply(&mut self, update: ItemUpdate) -> Result<(), Reason> {
        match update {
            ItemUpdate::Nothing => {}
            ItemUpdate::Set(out) => self.out = Some(out),
            ItemUpdate::Record(error) => self.error = Some(error),
            ItemUpdate::Reject => return Err(Reason::new("the item refuses this update")),
            ItemUpdate::Invalidate => self.out = Some(u32::MAX),
            ItemUpdate::Trigger(hook) => hook.0.trigger(),
            ItemUpdate::Explode => panic!("{APPLY_PANIC}"),
        }
        Ok(())
    }
}
