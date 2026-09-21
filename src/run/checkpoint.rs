use crate::run::cursor::Cursor;
use crate::state::State;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub state: State,
    pub cursor: Cursor,
}

impl Checkpoint {
    pub fn new(state: State, cursor: Cursor) -> Self {
        Self { state, cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::value::NodeId;

    #[test]
    fn given_checkpoint_when_round_tripped_then_identical() {
        let checkpoint = Checkpoint::new(
            State::empty(),
            Cursor::new(vec![NodeId::new("a").unwrap()], Vec::new()),
        );
        let json = serde_json::to_string(&checkpoint).unwrap();
        assert_eq!(
            serde_json::from_str::<Checkpoint>(&json).unwrap(),
            checkpoint
        );
    }
}
