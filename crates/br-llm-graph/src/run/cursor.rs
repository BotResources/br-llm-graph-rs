//! Where a run stands between two supersteps.

use crate::value::NodeId;

/// Where a run stands between two supersteps: the nodes to run next and the
/// join nodes deferred while other nodes are left to run. Its JSON is
/// `{"active", "deferred"}`.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    /// The nodes the next superstep runs.
    pub active: Vec<NodeId>,
    /// The join nodes targeted and deferred.
    pub deferred: Vec<NodeId>,
}

impl Cursor {
    /// The cursor of `active` and `deferred` nodes.
    pub fn new(active: Vec<NodeId>, deferred: Vec<NodeId>) -> Self {
        Self { active, deferred }
    }

    /// Whether no node is left to run.
    pub fn is_empty(&self) -> bool {
        self.active.is_empty() && self.deferred.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_cursor_when_round_tripped_then_identical() {
        let cursor = Cursor::new(
            vec![NodeId::new("a").unwrap()],
            vec![NodeId::new("j").unwrap()],
        );
        let json = serde_json::to_string(&cursor).unwrap();
        assert_eq!(serde_json::from_str::<Cursor>(&json).unwrap(), cursor);
    }
}
