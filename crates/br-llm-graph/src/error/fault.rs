use crate::error::GraphError;
use crate::graph::NodeError;

/// What failed a node (see `GraphError::NodeFailed`).
#[derive(Debug)]
pub enum NodeFault {
    /// The error the node returned, which a host can downcast to its own
    /// type.
    Returned(NodeError),
    /// The refusal of the node's updates.
    Refused(Box<GraphError>),
    /// The message of the node's panic.
    Panic(String),
}

impl std::fmt::Display for NodeFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeFault::Returned(error) => write!(f, "returned an error: {error}"),
            NodeFault::Refused(error) => write!(f, "its updates were refused: {error}"),
            NodeFault::Panic(message) => write!(f, "panicked: {message}"),
        }
    }
}

impl std::error::Error for NodeFault {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            NodeFault::Returned(error) => Some(error.as_ref()),
            NodeFault::Refused(error) => Some(error.as_ref()),
            NodeFault::Panic(_) => None,
        }
    }
}
