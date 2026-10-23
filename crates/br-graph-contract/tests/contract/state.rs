use std::collections::HashMap;
use std::num::NonZeroUsize;

use br_graph_contract::{ApplyError, Fields, Reason, State};
use serde::{Deserialize, Serialize};

use crate::common::{Tally, TallyUpdate, state, tally};

fn limit(bytes: usize) -> NonZeroUsize {
    NonZeroUsize::new(bytes).unwrap()
}

#[test]
fn given_valid_fields_when_created_then_the_state_exposes_them_with_the_default_limit() {
    let state = state(3);
    assert_eq!(state.fields(), &tally(3));
    assert_eq!(state.limit().get(), 131_072);
}

#[test]
fn given_invalid_fields_when_created_then_it_is_invalid_state_with_a_reason() {
    let error = State::create(tally(-1)).unwrap_err();
    assert_eq!(error.reason(), Reason::new("total is negative"));
}

#[test]
fn given_an_update_when_applied_then_a_new_state_is_given_and_the_first_is_unchanged() {
    let first = state(1);
    let second = first.apply(TallyUpdate::Add(4)).unwrap();
    assert_eq!(second.fields().total, 5);
    assert_eq!(first.fields().total, 1);
}

#[test]
fn given_an_update_the_fields_refuse_when_applied_then_it_is_refused_with_a_reason() {
    let first = state(1);
    let error = first.apply(TallyUpdate::Reject).unwrap_err();
    assert_eq!(
        error,
        ApplyError::Refused(Reason::new("the update is rejected"))
    );
    assert_eq!(first.fields().total, 1);
}

#[test]
fn given_an_update_that_gives_invalid_fields_when_applied_then_it_is_invalid_state() {
    let first = state(1);
    let error = first.apply(TallyUpdate::Add(-5)).unwrap_err();
    assert!(matches!(
        &error,
        ApplyError::InvalidState(invalid) if invalid.reason() == Reason::new("total is negative")
    ));
    assert_eq!(first.fields().total, 1);
}

#[test]
fn given_the_empty_update_when_applied_then_the_state_is_equal() {
    let first = state(7);
    let second = first.apply(Tally::empty_update()).unwrap();
    assert_eq!(first, second);
}

#[test]
fn given_a_state_when_serialized_and_rebuilt_then_it_is_equal() {
    let first = State::create(Tally {
        total: 9,
        notes: vec!["a".into(), "é".into()],
    })
    .unwrap();
    let json = first.to_json().unwrap();
    let rebuilt = State::<Tally>::rebuild(&json).unwrap();
    assert_eq!(first, rebuilt);
    assert_eq!(first.to_json().unwrap().len(), json.len());
}

#[test]
fn given_malformed_data_when_rebuilt_then_it_is_invalid_state() {
    for bad in [&b"not json"[..], b"{}", br#"{"total":"x","notes":[]}"#, b""] {
        let error = State::<Tally>::rebuild(bad).unwrap_err();
        assert!(!error.reason().as_str().is_empty());
    }
}

#[test]
fn given_data_with_invalid_fields_when_rebuilt_then_it_is_invalid_state() {
    let error = State::<Tally>::rebuild(br#"{"total":-3,"notes":[]}"#).unwrap_err();
    assert_eq!(error.reason(), Reason::new("total is negative"));
}

#[test]
fn given_fields_over_the_limit_when_created_then_the_reason_names_the_size_and_the_limit() {
    let fields = Tally {
        total: 0,
        notes: vec!["x".repeat(200)],
    };
    let size = serde_json::to_vec(&fields).unwrap().len();
    let error = State::create_with_limit(fields, limit(100)).unwrap_err();
    let reason = error.reason().to_string();
    assert!(reason.contains(&format!("{size} bytes")), "{reason}");
    assert!(reason.contains("limit of 100 bytes"), "{reason}");
}

#[test]
fn given_fields_exactly_at_the_limit_when_created_then_they_are_accepted() {
    let fields = tally(0);
    let size = serde_json::to_vec(&fields).unwrap().len();
    assert!(State::create_with_limit(fields.clone(), limit(size)).is_ok());
    assert!(State::create_with_limit(fields, limit(size - 1)).is_err());
}

#[test]
fn given_an_update_that_grows_past_the_limit_when_applied_then_it_is_invalid_state_and_unchanged() {
    let first = State::create_with_limit(tally(0), limit(60)).unwrap();
    let error = first.apply(TallyUpdate::Note("y".repeat(100))).unwrap_err();
    assert!(matches!(
        &error,
        ApplyError::InvalidState(invalid) if invalid.reason().as_str().contains("60")
    ));
    assert!(first.fields().notes.is_empty());
}

#[test]
fn given_a_limit_when_the_state_is_applied_then_the_next_state_keeps_it() {
    let first = State::create_with_limit(tally(0), limit(500)).unwrap();
    let second = first.apply(TallyUpdate::Add(1)).unwrap();
    assert_eq!(second.limit().get(), 500);
}

#[test]
fn given_data_over_the_limit_when_rebuilt_then_it_is_invalid_state() {
    let fields = Tally {
        total: 0,
        notes: vec!["x".repeat(200)],
    };
    let json = serde_json::to_vec(&fields).unwrap();
    let error = State::<Tally>::rebuild_with_limit(&json, limit(100)).unwrap_err();
    assert!(error.reason().as_str().contains("100"));
    assert!(State::<Tally>::rebuild_with_limit(&json, limit(10_000)).is_ok());
}

#[test]
fn given_a_large_state_when_rebuilt_with_the_default_limit_then_the_limit_is_128_kib() {
    let big = Tally {
        total: 0,
        notes: vec!["z".repeat(131_072)],
    };
    assert!(State::create(big.clone()).is_err());
    let json = serde_json::to_vec(&big).unwrap();
    assert!(State::<Tally>::rebuild(&json).is_err());
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct Unserializable {
    by_pair: HashMap<(u8, u8), u8>,
}

impl Fields for Unserializable {
    type Update = ();

    fn empty_update() {}

    fn validate(&self) -> Result<(), Reason> {
        Ok(())
    }

    fn apply(&mut self, _update: ()) -> Result<(), Reason> {
        Ok(())
    }
}

#[test]
fn given_fields_that_cannot_be_serialized_when_created_then_it_is_invalid_state_without_panic() {
    let mut by_pair = HashMap::new();
    by_pair.insert((1, 2), 3);
    let error = State::create(Unserializable { by_pair }).unwrap_err();
    assert!(error.reason().as_str().contains("serialized"));
}

#[test]
fn given_a_state_when_debug_printed_then_the_fields_are_not_printed() {
    let state = State::create(Tally {
        total: 0,
        notes: vec!["secret-note".into()],
    })
    .unwrap();
    assert!(!format!("{state:?}").contains("secret-note"));
}

#[test]
fn given_a_state_when_converted_into_its_fields_then_the_fields_come_out() {
    let state = State::create(Tally {
        total: 2,
        notes: vec!["kept".into()],
    })
    .unwrap();
    let fields = state.fields().clone();
    assert_eq!(fields.total, 2);
    assert_eq!(fields.notes, vec!["kept".to_owned()]);
}
