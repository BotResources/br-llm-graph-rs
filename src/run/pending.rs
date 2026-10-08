use std::collections::BTreeMap;

use crate::origin::OccurrenceKey;
use crate::update::Update;
use crate::value::NodeId;

/// The updates of finished occurrences whose superstep is still open, keyed
/// by occurrence. They travel in a checkpoint so that a resumed run does not
/// run those occurrences again. Serialized as a map from the written form of
/// each occurrence key to its updates.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct PendingWrites(BTreeMap<OccurrenceKey, Vec<Update>>);

impl PendingWrites {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the updates of a finished occurrence, replacing what was there.
    pub fn insert(&mut self, occurrence: OccurrenceKey, updates: Vec<Update>) {
        self.0.insert(occurrence, updates);
    }

    pub fn get(&self, occurrence: &OccurrenceKey) -> Option<&[Update]> {
        self.0.get(occurrence).map(Vec::as_slice)
    }

    /// Adds the entries of `other`; an occurrence present in both takes the
    /// updates of `other`.
    pub fn merge(&mut self, other: PendingWrites) {
        self.0.extend(other.0);
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&OccurrenceKey, &[Update])> {
        self.0
            .iter()
            .map(|(key, updates)| (key, updates.as_slice()))
    }

    /// The entries at or below `prefix`.
    pub(crate) fn under(&self, prefix: &OccurrenceKey) -> PendingWrites {
        Self(
            self.0
                .iter()
                .filter(|(key, _)| key.starts_with(prefix))
                .map(|(key, updates)| (key.clone(), updates.clone()))
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
