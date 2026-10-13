use std::collections::BTreeSet;

use super::*;

fn nid(name: &str) -> NodeId {
    NodeId::new(name).unwrap()
}

fn path() -> OccurrenceKey {
    OccurrenceKey::root()
        .child(Segment::node(nid("a")))
        .child(Segment::item(nid("b"), 3))
        .child(Segment::node(nid("c")))
}

#[test]
fn given_a_path_when_displayed_then_slash_separated_with_indices() {
    assert_eq!(path().to_string(), "a/b[3]/c");
    assert_eq!(OccurrenceKey::root().to_string(), "");
}

#[test]
fn given_a_displayed_path_when_parsed_then_identical() {
    assert_eq!(OccurrenceKey::parse("a/b[3]/c").unwrap(), path());
    assert_eq!(OccurrenceKey::parse("").unwrap(), OccurrenceKey::root());
}

#[test]
fn given_malformed_text_when_parsed_then_invalid_occurrence() {
    for text in ["a//b", "A", "a[", "a[]", "a[x]", "a[+1]", "a]", "/a", "a/"] {
        assert!(
            matches!(
                OccurrenceKey::parse(text),
                Err(GraphError::InvalidOccurrence { .. })
            ),
            "{text:?} was accepted"
        );
    }
}

#[test]
fn given_a_path_when_round_tripped_through_json_then_identical_and_written_as_text() {
    let json = serde_json::to_string(&path()).unwrap();
    assert_eq!(json, "\"a/b[3]/c\"");
    assert_eq!(
        serde_json::from_str::<OccurrenceKey>(&json).unwrap(),
        path()
    );
}

#[test]
fn given_paths_when_ordered_then_parents_first_and_indices_ascending() {
    let set: BTreeSet<OccurrenceKey> = ["a/b[10]", "a", "a/b[2]", "a/b", "c"]
        .iter()
        .map(|text| OccurrenceKey::parse(text).unwrap())
        .collect();
    let ordered: Vec<String> = set.iter().map(ToString::to_string).collect();
    assert_eq!(ordered, vec!["a", "a/b", "a/b[2]", "a/b[10]", "c"]);
}

#[test]
fn given_a_path_when_asked_for_an_item_then_the_last_segment_takes_the_index() {
    let node = OccurrenceKey::parse("a/m").unwrap();
    assert_eq!(node.item(4).unwrap().to_string(), "a/m[4]");
    let item = OccurrenceKey::parse("a/m[4]").unwrap();
    assert_eq!(item.item(1).unwrap().to_string(), "a/m[4]/m[1]");
    assert!(OccurrenceKey::root().item(0).is_none());
}

#[test]
fn given_a_prefix_when_checked_then_starts_with() {
    assert!(path().starts_with(&OccurrenceKey::parse("a/b[3]").unwrap()));
    assert!(path().starts_with(&OccurrenceKey::root()));
    assert!(!path().starts_with(&OccurrenceKey::parse("a/b").unwrap()));
}

#[test]
fn given_an_origin_when_round_tripped_then_identical() {
    let origin = Origin {
        run: RunId::new("run-7"),
        occurrence: path(),
    };
    let json = serde_json::to_string(&origin).unwrap();
    assert_eq!(serde_json::from_str::<Origin>(&json).unwrap(), origin);
    assert_eq!(origin.to_string(), "run-7:a/b[3]/c");
}
