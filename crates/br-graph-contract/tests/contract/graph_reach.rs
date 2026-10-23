use br_graph_contract::{BuildError, GraphBuilder};

use crate::common::{chain, edge, plain, refused};

#[test]
fn given_a_node_not_reachable_from_any_entry_when_built_then_it_is_refused() {
    let builder = chain().node(plain("island")).edge(edge("island", &["END"]));
    assert_eq!(
        refused(builder),
        BuildError::UnreachableNode {
            node: "island".into()
        }
    );
}

#[test]
fn given_a_node_reachable_only_from_a_second_entry_when_built_then_it_is_accepted() {
    let builder = chain()
        .node(plain("island"))
        .edge(edge("island", &["END"]))
        .entry("island");
    assert!(builder.build().is_ok());
}

#[test]
fn given_a_node_from_which_end_cannot_be_reached_when_built_then_it_is_refused() {
    let builder = GraphBuilder::new("1")
        .node(plain("start"))
        .node(plain("spin"))
        .entry("start")
        .edge(edge("start", &["spin", "END"]))
        .edge(edge("spin", &["spin"]));
    assert_eq!(
        refused(builder),
        BuildError::EndUnreachable {
            node: "spin".into()
        }
    );
}

#[test]
fn given_a_declared_end_only_through_a_cycle_when_built_then_every_node_reaches_end() {
    let builder = GraphBuilder::new("1")
        .node(plain("a"))
        .node(plain("b"))
        .entry("a")
        .edge(edge("a", &["b"]))
        .edge(edge("b", &["a", "END"]));
    assert!(builder.build().is_ok());
}
