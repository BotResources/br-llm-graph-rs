use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_graph_contract::{
    BuildError, CancelView, CreateFuture, Edge, Graph, GraphBuilder, InstanceId, Node, Target,
};

use crate::common::{
    FixedEdge, Tally, chain, edge, edge_answering, fixed_edge, plain, refused, single, targets,
};

#[test]
fn given_a_valid_chain_when_built_then_the_graph_gives_its_parts() {
    let graph: Graph<Tally> = chain().build().unwrap();
    assert_eq!(graph.version(), "1");
    assert_eq!(graph.entries(), ["first".to_owned()]);
    assert_eq!(graph.node("second").unwrap().name(), "second");
    assert!(graph.node("ghost").is_none());
    assert_eq!(graph.edge_of("second").unwrap().source(), "second");
    assert!(graph.edge_of("ghost").is_none());
}

#[test]
fn given_a_graph_with_a_loop_when_built_then_cycles_are_allowed() {
    let graph = GraphBuilder::new("1")
        .node(plain("work"))
        .node(plain("check"))
        .entry("work")
        .edge(edge("work", &["check"]))
        .edge(edge("check", &["work", "END"]))
        .build();
    assert!(graph.is_ok());
}

#[test]
fn given_several_entries_when_built_then_all_are_kept_sorted() {
    let graph = chain()
        .node(plain("alpha"))
        .edge(edge("alpha", &["second"]))
        .entry("alpha")
        .build()
        .unwrap();
    assert_eq!(graph.entries(), ["alpha".to_owned(), "first".to_owned()]);
}

#[test]
fn given_a_built_graph_when_used_by_many_threads_then_it_serves_them_all() {
    let graph = Arc::new(chain().build().unwrap());
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let graph = Arc::clone(&graph);
            std::thread::spawn(move || graph.node("first").is_some())
        })
        .collect();
    assert!(handles.into_iter().all(|handle| handle.join().unwrap()));
}

#[test]
fn given_two_nodes_with_one_name_when_built_then_it_is_refused() {
    let builder = chain().node(plain("second"));
    assert_eq!(
        refused(builder),
        BuildError::DuplicateNodeName {
            name: "second".into()
        }
    );
}

#[test]
fn given_two_edges_with_one_name_when_built_then_it_is_refused() {
    let builder = GraphBuilder::new("1")
        .node(plain("first"))
        .node(plain("second"))
        .entry("first")
        .edge(fixed_edge("shared", "first", &["second"]))
        .edge(fixed_edge("shared", "second", &["END"]));
    assert_eq!(
        refused(builder),
        BuildError::DuplicateEdgeName {
            name: "shared".into()
        }
    );
}

#[test]
fn given_no_nodes_when_built_then_it_is_refused() {
    let error = refused(GraphBuilder::new("1"));
    assert!(matches!(error, BuildError::NoNodes | BuildError::NoEntry));
    let with_entry = refused(GraphBuilder::<Tally>::new("1").entry("first"));
    assert!(matches!(
        with_entry,
        BuildError::NoNodes | BuildError::EntryNotANode { .. }
    ));
}

#[test]
fn given_no_entry_when_built_then_it_is_refused() {
    let builder = GraphBuilder::new("1")
        .node(plain("first"))
        .edge(edge("first", &["END"]));
    assert_eq!(refused(builder), BuildError::NoEntry);
}

#[test]
fn given_an_entry_listed_twice_when_built_then_it_is_refused() {
    assert_eq!(
        refused(chain().entry("first")),
        BuildError::DuplicateEntry {
            name: "first".into()
        }
    );
}

#[test]
fn given_an_entry_that_is_not_a_node_when_built_then_it_is_refused() {
    assert_eq!(
        refused(chain().entry("ghost")),
        BuildError::EntryNotANode {
            name: "ghost".into()
        }
    );
}

#[test]
fn given_an_edge_whose_source_is_not_a_node_when_built_then_it_is_refused() {
    let builder = chain().edge(fixed_edge("lost", "ghost", &["END"]));
    assert_eq!(
        refused(builder),
        BuildError::EdgeSourceNotANode {
            edge: "lost".into(),
            source_node: "ghost".into()
        }
    );
}

#[test]
fn given_a_node_with_no_edge_when_built_then_it_is_refused() {
    let builder = GraphBuilder::new("1")
        .node(plain("first"))
        .node(plain("second"))
        .entry("first")
        .edge(edge("first", &["second", "END"]));
    assert_eq!(
        refused(builder),
        BuildError::NodeWithoutEdge {
            node: "second".into()
        }
    );
}

#[test]
fn given_a_node_with_two_edges_when_built_then_it_is_refused() {
    let builder = chain().edge(fixed_edge("another", "first", &["second"]));
    assert_eq!(
        refused(builder),
        BuildError::NodeWithSeveralEdges {
            node: "first".into()
        }
    );
}

#[test]
fn given_an_empty_list_of_declared_targets_when_built_then_it_is_refused() {
    let builder = GraphBuilder::new("1")
        .node(plain("first"))
        .entry("first")
        .edge(fixed_edge("after_first", "first", &[]));
    assert_eq!(
        refused(builder),
        BuildError::EmptyTargets {
            edge: "after_first".into()
        }
    );
}

#[test]
fn given_a_declared_target_listed_twice_when_built_then_it_is_refused() {
    let builder = GraphBuilder::new("1")
        .node(plain("first"))
        .node(plain("second"))
        .entry("first")
        .edge(edge("first", &["second", "second"]))
        .edge(edge("second", &["END"]));
    assert_eq!(
        refused(builder),
        BuildError::DuplicateTarget {
            edge: "after_first".into(),
            target: Target::node("second")
        }
    );
}

#[test]
fn given_end_declared_twice_when_built_then_it_is_refused() {
    let builder = single(plain("first"), "first");
    let duplicated = GraphBuilder::new("1")
        .node(plain("first"))
        .entry("first")
        .edge(edge("first", &["END", "END"]));
    assert!(builder.build().is_ok());
    assert!(matches!(
        refused(duplicated),
        BuildError::DuplicateTarget { .. }
    ));
}

#[test]
fn given_a_declared_target_that_is_neither_a_node_nor_end_when_built_then_it_is_refused() {
    let builder = GraphBuilder::new("1")
        .node(plain("first"))
        .entry("first")
        .edge(edge("first", &["ghost", "END"]));
    assert_eq!(
        refused(builder),
        BuildError::UnknownTarget {
            edge: "after_first".into(),
            target: Target::node("ghost")
        }
    );
    let lowercase_end = GraphBuilder::new("1")
        .node(plain("first"))
        .entry("first")
        .edge(fixed_edge("after_first", "first", &["end"]));
    assert!(matches!(
        refused(lowercase_end),
        BuildError::UnknownTarget { .. }
    ));
}

struct Shifty {
    calls: AtomicUsize,
}

impl Node<Tally> for Shifty {
    fn name(&self) -> &str {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            "alpha"
        } else {
            "beta"
        }
    }

    fn kind(&self) -> &str {
        "programmatic"
    }

    fn subkind(&self) -> &str {
        "shifty"
    }

    fn create(&self, _cancel: CancelView, _id: InstanceId) -> CreateFuture<'_, Tally> {
        Box::pin(async { Err(br_graph_contract::CreateError::new("unused")) })
    }
}

#[test]
fn given_a_node_whose_name_changes_when_built_then_the_graph_keys_on_the_first_read() {
    let shifty: Arc<dyn Node<Tally>> = Arc::new(Shifty {
        calls: AtomicUsize::new(0),
    });
    let graph = GraphBuilder::new("1")
        .node(shifty)
        .entry("alpha")
        .edge(edge_answering("alpha", &["END"], Ok(targets(&["END"]))))
        .build()
        .unwrap();
    assert!(graph.node("alpha").is_some());
    assert!(graph.node("beta").is_none());
    assert_eq!(graph.structure().nodes()[0].name, "alpha");
}

#[test]
fn given_a_node_target_spelled_end_when_declared_then_it_is_not_the_end_target() {
    let declared = GraphBuilder::new("1")
        .node(plain("first"))
        .entry("first")
        .edge(Arc::new(FixedEdge {
            name: "after_first".into(),
            source: "first".into(),
            declared: vec![Target::Node("END".into())],
            answer: Ok(vec![Target::End]),
        }) as Arc<dyn Edge<Tally>>);
    assert!(matches!(
        refused(declared),
        BuildError::UnknownTarget { .. }
    ));
}
