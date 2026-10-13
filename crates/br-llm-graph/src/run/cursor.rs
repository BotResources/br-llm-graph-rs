use crate::value::NodeId;

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Cursor {
    pub active: Vec<NodeId>,
    pub deferred: Vec<NodeId>,
}

impl Cursor {
    pub fn new(active: Vec<NodeId>, deferred: Vec<NodeId>) -> Self {
        Self { active, deferred }
    }

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
