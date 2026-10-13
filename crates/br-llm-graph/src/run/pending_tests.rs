use super::*;
use crate::value::Key;

fn occurrence(text: &str) -> OccurrenceKey {
    OccurrenceKey::parse(text).unwrap()
}

fn append(text: &str) -> Vec<Update> {
    vec![Update::Append {
        key: Key::new("outs").unwrap(),
        value: Value::str(text),
    }]
}

fn entry(text: &str) -> PendingEntry {
    PendingEntry::new(append(text))
}

#[test]
fn given_inserted_entries_when_read_then_found_by_occurrence() {
    let mut pending = PendingWrites::new();
    pending.insert(occurrence("m[0]"), entry("a"));
    assert_eq!(pending.get(&occurrence("m[0]")), Some(&entry("a")));
    assert_eq!(pending.get(&occurrence("m[1]")), None);
    assert_eq!(pending.len(), 1);
}

#[test]
fn given_a_witnessed_entry_when_matched_then_only_the_same_witness_finds_it() {
    let mut pending = PendingWrites::new();
    pending.insert(
        occurrence("m[0]"),
        PendingEntry::witnessed(Value::str("x"), append("X")),
    );
    pending.insert(occurrence("n"), entry("node"));
    let x = Value::str("x");
    let y = Value::str("y");
    assert_eq!(
        pending.matching(&occurrence("m[0]"), Some(&x)),
        Some(append("X").as_slice())
    );
    assert_eq!(pending.matching(&occurrence("m[0]"), Some(&y)), None);
    assert_eq!(pending.matching(&occurrence("m[0]"), None), None);
    assert_eq!(
        pending.matching(&occurrence("n"), None),
        Some(append("node").as_slice())
    );
    assert_eq!(pending.matching(&occurrence("n"), Some(&x)), None);
}

#[test]
fn given_two_records_when_merged_then_union_and_the_merged_entry_wins() {
    let mut first = PendingWrites::new();
    first.insert(occurrence("m[0]"), entry("a"));
    first.insert(occurrence("m[1]"), entry("old"));
    let mut second = PendingWrites::new();
    second.insert(occurrence("m[1]"), entry("b"));
    second.insert(occurrence("m[2]"), entry("c"));
    first.merge(second);
    let entries: Vec<String> = first.iter().map(|(key, _)| key.to_string()).collect();
    assert_eq!(entries, vec!["m[0]", "m[1]", "m[2]"]);
    assert_eq!(first.get(&occurrence("m[1]")), Some(&entry("b")));
}

#[test]
fn given_entries_when_round_tripped_then_identical_and_keyed_by_written_occurrence() {
    let mut pending = PendingWrites::new();
    pending.insert(
        occurrence("call/m[3]"),
        PendingEntry::witnessed(Value::str("a"), append("A")),
    );
    pending.insert(occurrence("call"), entry("b"));
    let json = serde_json::to_value(&pending).unwrap();
    assert!(json["call/m[3]"].get("witness").is_some());
    assert!(json["call"].get("witness").is_none());
    assert_eq!(
        serde_json::from_value::<PendingWrites>(json).unwrap(),
        pending
    );
}

#[test]
fn given_entries_of_several_nodes_when_a_superstep_completes_then_only_its_nodes_are_dropped() {
    let mut pending = PendingWrites::new();
    for key in ["m[0]", "m[1]/x[2]", "n[0]", "call/m[0]", "mm[0]"] {
        pending.insert(occurrence(key), entry(key));
    }
    pending.drop_nodes(&OccurrenceKey::root(), &[NodeId::new("m").unwrap()]);
    let left: Vec<String> = pending.iter().map(|(key, _)| key.to_string()).collect();
    assert_eq!(left, vec!["call/m[0]", "mm[0]", "n[0]"]);
    pending.drop_nodes(&occurrence("call"), &[NodeId::new("m").unwrap()]);
    assert_eq!(pending.len(), 2);
    assert_eq!(pending.under(&OccurrenceKey::root()).len(), 2);
}

#[test]
fn given_entries_around_an_occurrence_when_dropped_below_then_only_deeper_ones_go() {
    let mut pending = PendingWrites::new();
    for key in ["call", "call/a", "call/m[1]/b", "callee/a", "m[0]"] {
        pending.insert(occurrence(key), entry(key));
    }
    pending.drop_below(&occurrence("call"));
    let left: Vec<String> = pending.iter().map(|(key, _)| key.to_string()).collect();
    assert_eq!(left, vec!["call", "callee/a", "m[0]"]);
}
