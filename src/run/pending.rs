use std::collections::BTreeMap;

use crate::origin::OccurrenceKey;
use crate::state::Value;
use crate::update::Update;
use crate::value::NodeId;

/// The updates a finished occurrence returned. A map item also keeps the item
/// it ran on (its witness): the entry stands for that item only.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PendingEntry {
    pub updates: Vec<Update>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub witness: Option<Value>,
}

impl PendingEntry {
    /// The entry of a node occurrence.
    pub fn new(updates: Vec<Update>) -> Self {
        Self {
            updates,
            witness: None,
        }
    }

    /// The entry of a map item that ran on `witness`.
    pub fn witnessed(witness: Value, updates: Vec<Update>) -> Self {
        Self {
            updates,
            witness: Some(witness),
        }
    }
}

/// The entries of finished occurrences whose superstep is still open, keyed
/// by occurrence. They travel in a checkpoint so that a resumed run does not
/// run those occurrences again. Serialized as a map from the written form of
/// each occurrence key to its entry.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct PendingWrites(BTreeMap<OccurrenceKey, PendingEntry>);

impl PendingWrites {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the entry of a finished occurrence, replacing what was there.
    pub fn insert(&mut self, occurrence: OccurrenceKey, entry: PendingEntry) {
        self.0.insert(occurrence, entry);
    }

    pub fn get(&self, occurrence: &OccurrenceKey) -> Option<&PendingEntry> {
        self.0.get(occurrence)
    }

    /// Adds the entries of `other`; an occurrence present in both takes the
    /// entry of `other`.
    pub fn merge(&mut self, other: PendingWrites) {
        self.0.extend(other.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&OccurrenceKey, &PendingEntry)> {
        self.0.iter()
    }

    pub(crate) fn remove(&mut self, occurrence: &OccurrenceKey) {
        self.0.remove(occurrence);
    }

    /// The updates recorded for `occurrence` when its witness is `witness`.
    pub(crate) fn matching(
        &self,
        occurrence: &OccurrenceKey,
        witness: Option<&Value>,
    ) -> Option<&[Update]> {
        self.0
            .get(occurrence)
            .filter(|entry| entry.witness.as_ref() == witness)
            .map(|entry| entry.updates.as_slice())
    }

    /// The entries at or below `prefix`.
    pub(crate) fn under(&self, prefix: &OccurrenceKey) -> PendingWrites {
        Self(
            self.0
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(key, entry)| (key.clone(), entry.clone()))
                .collect(),
        )
    }

    /// Drops the entries of the occurrences of `nodes` run directly under
    /// `run`, whatever their item index, and everything nested in them.
    pub(crate) fn drop_nodes(&mut self, run: &OccurrenceKey, nodes: &[NodeId]) {
        let depth = run.segments().len();
        self.0.retain(|key, _| {
            if !key.starts_with(run) {
                return true;
            }
            match key.segments().get(depth) {
                Some(segment) => !nodes.contains(&segment.node),
                None => true,
            }
        });
    }
}

#[cfg(test)]
#[path = "pending_tests.rs"]
mod tests;
