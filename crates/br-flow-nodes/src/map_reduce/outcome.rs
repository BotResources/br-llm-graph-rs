use br_graph_contract::{Fields, State};

#[derive(Debug, Clone, PartialEq)]
pub enum ElementOutcome<G: Fields> {
    Done(State<G>),
    Failed(String),
}

impl<G: Fields> ElementOutcome<G> {
    #[must_use]
    pub fn state(&self) -> Option<&State<G>> {
        match self {
            Self::Done(state) => Some(state),
            Self::Failed(_) => None,
        }
    }

    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        match self {
            Self::Done(_) => None,
            Self::Failed(text) => Some(text),
        }
    }
}
