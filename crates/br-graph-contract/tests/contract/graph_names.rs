use br_graph_contract::{BuildError, GraphBuilder};

use crate::common::{custom_node, edge, fixed_edge, plain, refused, single};

#[test]
fn given_an_invalid_version_when_built_then_it_is_refused() {
    for version in ["", "has space", "é", &"v".repeat(65), "a/b"] {
        let builder = GraphBuilder::new(version)
            .node(plain("first"))
            .entry("first")
            .edge(edge("first", &["END"]));
        assert!(matches!(
            refused(builder),
            BuildError::InvalidVersion { .. }
        ));
    }
}

#[test]
fn given_valid_versions_when_built_then_they_are_accepted() {
    for version in ["1", "2.0.1-rc.1_x", "A", &"v".repeat(64)] {
        let builder = GraphBuilder::new(version)
            .node(plain("first"))
            .entry("first")
            .edge(edge("first", &["END"]));
        assert!(builder.build().is_ok(), "{version}");
    }
}

#[test]
fn given_an_invalid_node_name_when_built_then_it_is_refused() {
    for name in ["Bad", "1a", "", "a-b", "END", "a b", &"n".repeat(65), "é"] {
        let error = refused(single(plain(name), name));
        assert!(
            matches!(&error, BuildError::InvalidNodeName { name: found } if found == name),
            "{name}: {error:?}"
        );
    }
}

#[test]
fn given_valid_node_names_when_built_then_they_are_accepted() {
    for name in ["a", "a1_b2", &"n".repeat(64)] {
        assert!(single(plain(name), name).build().is_ok(), "{name}");
    }
}

#[test]
fn given_an_invalid_node_kind_when_built_then_it_is_refused() {
    for kind in ["Programmatic", "", "1x", &"k".repeat(33), "a-b"] {
        let error = refused(single(custom_node("n", kind, "worker"), "n"));
        assert!(
            matches!(error, BuildError::InvalidNodeKind { .. }),
            "{kind}"
        );
    }
    assert!(
        single(custom_node("n", &"k".repeat(32), "worker"), "n")
            .build()
            .is_ok()
    );
}

#[test]
fn given_an_invalid_node_subkind_when_built_then_it_is_refused() {
    for subkind in ["Merger", "", "_x", &"s".repeat(65), "a b"] {
        let error = refused(single(custom_node("n", "programmatic", subkind), "n"));
        assert!(
            matches!(error, BuildError::InvalidNodeSubkind { .. }),
            "{subkind}"
        );
    }
    assert!(
        single(custom_node("n", "programmatic", &"s".repeat(64)), "n")
            .build()
            .is_ok()
    );
}

#[test]
fn given_an_invalid_edge_name_when_built_then_it_is_refused() {
    for name in ["Bad", "", "a-b", &"e".repeat(65)] {
        let builder = GraphBuilder::new("1")
            .node(plain("n"))
            .entry("n")
            .edge(fixed_edge(name, "n", &["END"]));
        assert!(matches!(
            refused(builder),
            BuildError::InvalidEdgeName { .. }
        ));
    }
}
