use crate::error::GraphError;
use crate::graph::NodeError;

#[derive(Debug)]
pub enum NodeFault {
    Returned(NodeError),
    Refused(Box<GraphError>),
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
