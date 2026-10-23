use br_graph_contract::{Structure, StructureEdge, StructureError, StructureNode, Target};
use serde_json::{Value, json};

use crate::common::chain;

fn chain_json() -> Value {
    serde_json::to_value(chain().build().unwrap().structure()).unwrap()
}

#[test]
fn given_the_positional_array_form_when_rebuilt_then_it_is_refused_as_malformed() {
    let positional = r#"["v",["a"],[["a","k","s"]],[["e","a",["END"]]]]"#;
    assert!(matches!(
        Structure::from_json_str(positional).unwrap_err(),
        StructureError::Malformed(_)
    ));
    assert!(serde_json::from_str::<Structure>(positional).is_err());

    let mut array_node = chain_json();
    array_node["nodes"][0] = json!(["first", "programmatic", "worker"]);
    let mut array_edge = chain_json();
    array_edge["edges"][0] = json!(["after_first", "first", ["second"]]);
    for bad in [array_node, array_edge] {
        let text = bad.to_string();
        assert!(matches!(
            Structure::from_json_str(&text).unwrap_err(),
            StructureError::Malformed(_)
        ));
        assert!(serde_json::from_str::<Structure>(&text).is_err());
    }
}

#[test]
fn given_a_duplicate_missing_or_unknown_key_in_any_object_when_rebuilt_then_it_is_malformed() {
    let duplicate_version =
        r#"{"version":"1","version":"2","entries":["a"],"nodes":[],"edges":[]}"#;
    let duplicate_node_key = chain_json().to_string().replacen(
        r#""name":"first""#,
        r#""name":"first","name":"again""#,
        1,
    );
    let duplicate_edge_key = chain_json().to_string().replacen(
        r#""source":"first""#,
        r#""source":"first","source":"second""#,
        1,
    );
    let mut missing_node_key = chain_json();
    missing_node_key["nodes"][1]
        .as_object_mut()
        .unwrap()
        .remove("subkind");
    let mut missing_edge_key = chain_json();
    missing_edge_key["edges"][1]
        .as_object_mut()
        .unwrap()
        .remove("targets");
    let mut unknown_edge_key = chain_json();
    unknown_edge_key["edges"][0]["extra"] = json!(true);
    for bad in [
        duplicate_version.to_owned(),
        duplicate_node_key,
        duplicate_edge_key,
        missing_node_key.to_string(),
        missing_edge_key.to_string(),
        unknown_edge_key.to_string(),
    ] {
        assert!(
            matches!(
                Structure::from_json_str(&bad).unwrap_err(),
                StructureError::Malformed(_)
            ),
            "{bad}"
        );
        assert!(serde_json::from_str::<Structure>(&bad).is_err(), "{bad}");
    }
}

#[test]
fn given_a_node_or_an_edge_alone_when_deserialized_then_only_the_object_form_is_accepted() {
    let node: StructureNode =
        serde_json::from_str(r#"{"name":"a","kind":"programmatic","subkind":"worker"}"#).unwrap();
    assert_eq!(node.name, "a");
    assert!(serde_json::from_str::<StructureNode>(r#"["a","programmatic","worker"]"#).is_err());
    let edge: StructureEdge =
        serde_json::from_str(r#"{"name":"e","source":"a","targets":["END"]}"#).unwrap();
    assert_eq!(edge.targets, vec![Target::End]);
    assert!(serde_json::from_str::<StructureEdge>(r#"["e","a",["END"]]"#).is_err());
}
