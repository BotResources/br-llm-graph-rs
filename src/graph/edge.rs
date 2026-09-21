use crate::error::GraphError;
use crate::state::{Config, State};
use crate::value::{EndLabel, NodeId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Node(NodeId),
    End(EndLabel),
}

pub trait Edge: Send + Sync {
    fn next(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError>;
}

pub struct FnEdge<F>(F);

impl<F> FnEdge<F>
where
    F: Fn(&State, &Config) -> Result<Vec<Target>, GraphError> + Send + Sync,
{
    pub fn new(f: F) -> Self {
        Self(f)
    }
}

impl<F> Edge for FnEdge<F>
where
    F: Fn(&State, &Config) -> Result<Vec<Target>, GraphError> + Send + Sync,
{
    fn next(&self, state: &State, config: &Config) -> Result<Vec<Target>, GraphError> {
        (self.0)(state, config)
    }
}

pub struct Always(pub Target);

impl Edge for Always {
    fn next(&self, _state: &State, _config: &Config) -> Result<Vec<Target>, GraphError> {
        Ok(vec![self.0.clone()])
    }
}
