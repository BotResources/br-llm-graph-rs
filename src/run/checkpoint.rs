use crate::run::cursor::Cursor;
use crate::run::pending::PendingWrites;
use crate::state::State;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub state: State,
    pub cursor: Cursor,
    /// Updates of occurrences that finished while their superstep was still
    /// open. Absent from checkpoints written before it existed.
    #[serde(default, skip_serializing_if = "PendingWrites::is_empty")]
    pub pending: PendingWrites,
}

impl Checkpoint {
    pub fn new(state: State, cursor: Cursor) -> Self {
        Self {
            state,
            cursor,
            pending: PendingWrites::default(),
        }
    }

    pub fn with_pending(mut self, pending: PendingWrites) -> Self {
        self.pending = pending;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::origin::OccurrenceKey;
    use crate::state::{State, Value};
    use crate::update::Update;
    use crate::value::{Key, NodeId};

    fn checkpoint() -> Checkpoint {
        Checkpoint::new(
            State::empty(),
            Cursor::new(vec![NodeId::new("a").unwrap()], Vec::new()),
        )
    }

    #[test]
    fn given_checkpoint_when_round_tripped_then_identical() {
        let checkpoint = checkpoint();
        let json = serde_json::to_string(&checkpoint).unwrap();
        assert_eq!(
            serde_json::from_str::<Checkpoint>(&json).unwrap(),
            checkpoint
        );
    }

    #[test]
    fn given_checkpoint_with_pending_writes_when_round_tripped_then_identical() {
        let mut pending = PendingWrites::new();
        pending.insert(
            OccurrenceKey::parse("a/m[2]").unwrap(),
            vec![Update::Append {
                key: Key::new("outs").unwrap(),
                value: Value::str("x"),
            }],
        );
        let checkpoint = checkpoint().with_pending(pending);
        let json = serde_json::to_string(&checkpoint).unwrap();
        assert_eq!(
            serde_json::from_str::<Checkpoint>(&json).unwrap(),
            checkpoint
        );
    }

    #[test]
    fn given_checkpoint_written_without_pending_writes_when_read_then_none_pending() {
        let json = serde_json::json!({
            "state": serde_json::to_value(State::empty()).unwrap(),
            "cursor": { "active": ["a"], "deferred": [] }
        });
        let checkpoint = serde_json::from_value::<Checkpoint>(json).unwrap();
        assert!(checkpoint.pending.is_empty());
        assert_eq!(checkpoint, self::checkpoint());
    }
}
