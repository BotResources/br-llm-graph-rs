#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error("no node to start: the list of nodes is empty")]
    NoNodes,
    #[error("cannot start node {name:?}: the graph holds no such node")]
    UnknownNode { name: String },
    #[error("cannot start node {name:?}: it is listed twice")]
    DuplicateNode { name: String },
}

#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    #[error("checkpoint version {checkpoint:?} differs from graph version {graph:?}")]
    VersionMismatch { checkpoint: String, graph: String },
    #[error("checkpoint structure differs from the graph: {reason}")]
    StructureMismatch { reason: String },
    #[error("checkpoint state cannot be rebuilt: {reason}")]
    StateRejected { reason: String },
    #[error("checkpoint names node {name:?}, which the graph does not hold")]
    UnknownNode { name: String },
    #[error("checkpoint names node {name:?} twice")]
    DuplicateNode { name: String },
    #[error("checkpoint names no node")]
    NoNodes,
    #[error("checkpoint cannot be rebuilt: {reason}")]
    Malformed { reason: String },
}
