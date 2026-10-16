//! Applying updates: `Set` and `Append` with their refusals, a batch all or
//! nothing, two `Set` of one key in one batch refused before anything.

use std::collections::BTreeMap;

use br_llm_messages::Conversation;

use crate::error::GraphError;
use crate::state::kind::Kind;
use crate::state::schema::Schema;
use crate::state::state::State;
use crate::state::value::Value;
use crate::update::Update;
use crate::value::Key;

use super::conversation_tests::{input, turn};

pub(super) fn key(name: &str) -> Key {
    Key::new(name).unwrap()
}

pub(super) fn state() -> State {
    let schema = Schema::builder()
        .state_by_name(key("count"), Kind::Int)
        .state_by_name(key("items"), Kind::list(Kind::Str))
        .state_by_name(key("chat"), Kind::Conversation)
        .build();
    let mut values = BTreeMap::new();
    values.insert(key("count"), Value::int(0));
    values.insert(key("items"), Value::list(Vec::new()));
    values.insert(key("chat"), Value::conversation(Conversation::new()));
    State::new(schema, values).unwrap()
}

#[test]
fn given_set_when_kind_matches_then_applied() {
    let mut state = state();
    state
        .apply_batch(&[Update::Set {
            key: key("count"),
            value: Value::int(5),
        }])
        .unwrap();
    assert_eq!(state.int(&key("count")).unwrap(), 5);
}

#[test]
fn given_set_when_kind_wrong_then_refused_and_unchanged() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[Update::Set {
            key: key("count"),
            value: Value::str("no"),
        }]),
        Err(GraphError::KindMismatch { .. })
    ));
    assert_eq!(state.int(&key("count")).unwrap(), 0);
}

#[test]
fn given_append_when_list_then_pushed() {
    let mut state = state();
    state
        .apply_batch(&[Update::Append {
            key: key("items"),
            value: Value::str("a"),
        }])
        .unwrap();
    assert_eq!(state.list(&key("items")).unwrap().len(), 1);
}

#[test]
fn given_append_on_non_list_when_applied_then_refused() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[Update::Append {
            key: key("count"),
            value: Value::int(1),
        }]),
        Err(GraphError::AppendNotList { .. })
    ));
}

#[test]
fn given_two_sets_on_same_key_when_batched_then_conflict() {
    let mut state = state();
    assert!(matches!(
        state.apply_batch(&[
            Update::Set {
                key: key("count"),
                value: Value::int(1),
            },
            Update::Set {
                key: key("count"),
                value: Value::int(2),
            },
        ]),
        Err(GraphError::SetConflict { .. })
    ));
}

#[test]
fn given_two_appends_on_same_key_when_batched_then_both_applied() {
    let mut state = state();
    state
        .apply_batch(&[
            Update::Append {
                key: key("items"),
                value: Value::str("a"),
            },
            Update::Append {
                key: key("items"),
                value: Value::str("b"),
            },
        ])
        .unwrap();
    assert_eq!(state.list(&key("items")).unwrap().len(), 2);
}

#[test]
fn given_batch_with_a_late_failure_when_applied_then_all_or_nothing() {
    let mut state = state();
    let outcome = state.apply_batch(&[
        Update::Set {
            key: key("count"),
            value: Value::int(7),
        },
        Update::Append {
            key: key("count"),
            value: Value::int(1),
        },
    ]);
    assert!(outcome.is_err());
    assert_eq!(state.int(&key("count")).unwrap(), 0);
    assert!(state.list(&key("items")).unwrap().is_empty());
}

#[test]
fn given_an_update_of_an_undeclared_key_when_applied_then_unknown_key_and_state_unchanged() {
    let stray = key("stray");
    let updates = [
        Update::Set {
            key: stray.clone(),
            value: Value::int(1),
        },
        Update::Append {
            key: stray.clone(),
            value: Value::int(1),
        },
        Update::Input {
            key: stray.clone(),
            input: input("hi"),
        },
        Update::PushTurn {
            key: stray.clone(),
            turn: turn("t1"),
        },
    ];
    for update in updates {
        let mut state = state();
        let before = state.clone();
        let error = state.apply_batch(&[update]).err();
        assert!(
            matches!(error, Some(GraphError::UnknownKey { ref key }) if *key == stray),
            "{error:?}"
        );
        assert_eq!(state, before);
    }
}
