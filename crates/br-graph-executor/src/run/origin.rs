use crate::Checkpoint;

pub(crate) enum Origin {
    Nodes(Vec<String>),
    Checkpoint(Checkpoint),
}

impl Origin {
    pub(super) fn held_names(&self) -> &[String] {
        match self {
            Self::Nodes(nodes) => nodes,
            Self::Checkpoint(checkpoint) => checkpoint.nodes(),
        }
    }

    pub(super) fn first_targets(&self) -> &[String] {
        match self {
            Self::Nodes(nodes) => nodes,
            Self::Checkpoint(_) => &[],
        }
    }

    pub(super) fn into_checkpoint(self) -> Option<Checkpoint> {
        match self {
            Self::Nodes(_) => None,
            Self::Checkpoint(checkpoint) => Some(checkpoint),
        }
    }
}
