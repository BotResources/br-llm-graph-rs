use br_graph_contract::{Fields, Node};

pub const AGENTIC: &str = "agentic";

pub trait AgenticNode<F: Fields>: Node<F> {
    fn has_inbox(&self) -> bool;
}
