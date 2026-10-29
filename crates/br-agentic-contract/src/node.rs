use br_graph_contract::{Fields, Node};

pub const AGENTIC: &str = "agentic";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Restart {
    Resets,
    Keeps,
    NoCounters,
}

pub trait AgenticNode<F: Fields>: Node<F> {
    fn has_inbox(&self) -> bool;

    fn restart(&self) -> Restart;
}
