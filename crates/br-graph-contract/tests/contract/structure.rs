use br_graph_contract::{BuildError, GraphBuilder, Structure, StructureError, Target};
use serde_json::{Value, json};

use crate::common::{Tally, chain, edge, plain};

fn chain_json() -> Value {
    json!({
        "version": "1",
        "entries": ["first"],
        "nodes": [
            {"name": "first", "kind": "programmatic", "subkind": "worker"},
            {"name": "second", "kind": "programmatic", "subkind": "worker"},
            {"name": "third", "kind": "programmatic", "subkind": "worker"}
        ],
        "edges": [
            {"name": "after_first", "source": "first", "targets": ["second"]},
            {"name": "after_second", "source": "second", "targets": ["third"]},
            {"name": "after_third", "source": "third", "targets": ["END"]}
        ]
    })
}

#[test]
fn given_a_graph_when_described_then_the_structure_has_the_exact_field_names() {
    let structure = chain().build().unwrap().structure();
    assert_eq!(serde_json::to_value(&structure).unwrap(), chain_json());
}

#[test]
fn given_a_structure_when_read_then_the_accessors_give_its_parts() {
    let structure = chain().build().unwrap().structure();
    assert_eq!(structure.version(), "1");
    assert_eq!(structure.entries(), ["first".to_owned()]);
    assert_eq!(structure.nodes().len(), 3);
    assert_eq!(structure.nodes()[0].kind, "programmatic");
    assert_eq!(structure.edges()[2].targets, vec![Target::End]);
}

#[test]
fn given_two_builds_in_different_orders_when_described_then_the_structures_are_equal() {
    let forward = chain().build().unwrap().structure();
    let backward = GraphBuilder::<Tally>::new("1")
        .edge(edge("third", &["END"]))
        .edge(edge("second", &["third"]))
        .edge(edge("first", &["second"]))
        .node(plain("third"))
        .node(plain("second"))
        .node(plain("first"))
        .entry("first")
        .build()
        .unwrap()
        .structure();
    assert_eq!(forward, backward);
    assert_eq!(
        serde_json::to_vec(&forward).unwrap(),
        serde_json::to_vec(&backward).unwrap()
    );
}

#[test]
fn given_several_entries_when_described_then_they_are_sorted() {
    let structure = chain()
        .node(plain("alpha"))
        .edge(edge("alpha", &["END"]))
        .entry("third")
        .entry("alpha")
        .build()
        .unwrap()
        .structure();
    assert_eq!(
        structure.entries(),
        ["alpha".to_owned(), "first".to_owned(), "third".to_owned()]
    );
}

#[test]
fn given_declared_targets_when_described_then_they_keep_their_declared_order() {
    let structure = GraphBuilder::<Tally>::new("1")
        .node(plain("start"))
        .node(plain("alpha"))
        .node(plain("beta"))
        .entry("start")
        .edge(edge("start", &["beta", "END", "alpha"]))
        .edge(edge("alpha", &["END"]))
        .edge(edge("beta", &["END"]))
        .build()
        .unwrap()
        .structure();
    assert_eq!(
        structure.edges()[2].targets,
        vec![Target::node("beta"), Target::End, Target::node("alpha")]
    );
}

#[test]
fn given_a_serialized_structure_when_rebuilt_then_it_equals_the_original() {
    let structure = chain().build().unwrap().structure();
    let json = serde_json::to_string(&structure).unwrap();
    assert_eq!(Structure::from_json_str(&json).unwrap(), structure);
    assert_eq!(
        Structure::from_json_slice(json.as_bytes()).unwrap(),
        structure
    );
    let from_serde: Structure = serde_json::from_str(&json).unwrap();
    assert_eq!(from_serde, structure);
}

#[test]
fn given_unsorted_json_when_rebuilt_then_nodes_edges_and_entries_are_sorted() {
    let mut shuffled = chain_json();
    shuffled["nodes"].as_array_mut().unwrap().reverse();
    shuffled["edges"].as_array_mut().unwrap().reverse();
    shuffled["entries"] = json!(["third", "first"]);
    let mut expected = chain_json();
    expected["entries"] = json!(["first", "third"]);
    let rebuilt = Structure::from_json_str(&shuffled.to_string()).unwrap();
    assert_eq!(serde_json::to_value(&rebuilt).unwrap(), expected);
}

#[test]
fn given_targets_in_json_when_rebuilt_then_their_order_is_kept() {
    let mut json = chain_json();
    json["nodes"].as_array_mut().unwrap().push(json!(
        {"name": "side", "kind": "programmatic", "subkind": "worker"}
    ));
    json["edges"].as_array_mut().unwrap().push(json!(
        {"name": "after_side", "source": "side", "targets": ["END"]}
    ));
    json["edges"][0]["targets"] = json!(["second", "side"]);
    let rebuilt = Structure::from_json_str(&json.to_string()).unwrap();
    assert_eq!(
        rebuilt.edges()[0].targets,
        vec![Target::node("second"), Target::node("side")]
    );
}

#[test]
fn given_malformed_data_when_rebuilt_then_it_is_refused_with_a_reason() {
    let missing_field = {
        let mut json = chain_json();
        json.as_object_mut().unwrap().remove("version");
        json.to_string()
    };
    let wrong_type = {
        let mut json = chain_json();
        json["entries"] = json!("first");
        json.to_string()
    };
    let unknown_field = {
        let mut json = chain_json();
        json["extra"] = json!(1);
        json.to_string()
    };
    let unknown_node_field = {
        let mut json = chain_json();
        json["nodes"][0]["extra"] = json!(1);
        json.to_string()
    };
    for bad in [
        "not json".to_owned(),
        String::new(),
        "[]".to_owned(),
        missing_field,
        wrong_type,
        unknown_field,
        unknown_node_field,
    ] {
        let error = Structure::from_json_str(&bad).unwrap_err();
        assert!(matches!(error, StructureError::Malformed(_)), "{bad}");
        assert!(!error.reason().as_str().is_empty());
    }
}

#[test]
fn given_json_that_breaks_a_build_rule_when_rebuilt_then_it_is_refused_with_the_rule() {
    let mut duplicate_node = chain_json();
    let first = duplicate_node["nodes"][0].clone();
    duplicate_node["nodes"].as_array_mut().unwrap().push(first);
    assert_eq!(
        Structure::from_json_str(&duplicate_node.to_string()).unwrap_err(),
        StructureError::Rule(BuildError::DuplicateNodeName {
            name: "first".into()
        })
    );

    let mut unknown_target = chain_json();
    unknown_target["edges"][0]["targets"] = json!(["ghost"]);
    assert!(matches!(
        Structure::from_json_str(&unknown_target.to_string()).unwrap_err(),
        StructureError::Rule(BuildError::UnknownTarget { .. })
    ));

    let mut no_entry = chain_json();
    no_entry["entries"] = json!([]);
    let error = Structure::from_json_str(&no_entry.to_string()).unwrap_err();
    assert_eq!(error, StructureError::Rule(BuildError::NoEntry));
    assert!(!error.reason().as_str().is_empty());
}

#[test]
fn given_the_same_shape_when_built_or_rebuilt_then_both_refuse_it_for_the_same_rule() {
    let cases: Vec<(Value, GraphBuilder<Tally>)> = vec![
        (
            {
                let mut json = chain_json();
                json["version"] = json!("bad version");
                json
            },
            GraphBuilder::new("bad version")
                .node(plain("first"))
                .edge(edge("first", &["END"]))
                .entry("first"),
        ),
        (
            {
                let mut json = chain_json();
                json["entries"] = json!(["first", "first"]);
                json
            },
            chain().entry("first"),
        ),
        (
            {
                let mut json = chain_json();
                json["entries"] = json!(["first", "ghost"]);
                json
            },
            chain().entry("ghost"),
        ),
        (
            {
                let mut json = chain_json();
                json["edges"][0]["targets"] = json!([]);
                json
            },
            GraphBuilder::new("1")
                .node(plain("first"))
                .entry("first")
                .edge(crate::common::edge_answering("first", &[], Ok(vec![]))),
        ),
    ];
    for (json, builder) in cases {
        let from_json = Structure::from_json_str(&json.to_string()).unwrap_err();
        let from_builder = builder.build().unwrap_err();
        assert!(
            matches!(&from_json, StructureError::Rule(rule) if std::mem::discriminant(rule) == std::mem::discriminant(&from_builder)),
            "{from_json:?} vs {from_builder:?}"
        );
    }
}
