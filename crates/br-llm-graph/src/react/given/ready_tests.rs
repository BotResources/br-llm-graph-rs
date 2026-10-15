//! The ready-made subjects: the texts they give, the keys they declare, and
//! the client each schedule gives for each call.

use std::collections::BTreeMap;
use std::sync::Arc;

use br_llm_clients_core::Client;

use crate::error::GraphError;
use crate::react::given::ready::{from_start, with_tail};
use crate::react::given::{Model, Prompt, Reads};
use crate::react::llm_call::LlmCall;
use crate::react::test_support::{ckey, key, mock, skey};
use crate::state::{Config, Kind, Schema, State, Value};

/// A state of strings and lists of strings, and a configuration of both.
fn texts() -> (State, Config) {
    let schema = Schema::builder()
        .state_by_name(key("topic"), Kind::Str)
        .state_by_name(key("rules"), Kind::list(Kind::Str))
        .state_by_name(key("count"), Kind::Int)
        .config_by_name(key("tone"), Kind::Str)
        .config_by_name(key("notes"), Kind::list(Kind::Str))
        .build();
    let state = State::new(
        schema.clone(),
        BTreeMap::from([
            (key("topic"), Value::str("tides")),
            (
                key("rules"),
                Value::list(vec![Value::str("Be brief."), Value::str("Cite.")]),
            ),
            (key("count"), Value::int(3)),
        ]),
    )
    .unwrap();
    let config = Config::new(
        &schema,
        BTreeMap::from([
            (key("tone"), Value::str("plain")),
            (key("notes"), Value::list(Vec::new())),
        ]),
    )
    .unwrap();
    (state, config)
}

fn given(prompt: &Prompt<LlmCall>) -> String {
    let (state, config) = texts();
    prompt.give(&state, &config, ()).unwrap()
}

#[test]
fn given_a_fixed_text_when_given_then_the_text_whatever_the_state_and_no_key_read() {
    let prompt = Prompt::text("Summarize.");
    assert_eq!(given(&prompt), "Summarize.");
    assert_eq!(prompt.reads(), &Reads::new());
}

#[test]
fn given_texts_read_from_keys_when_given_then_a_string_or_the_items_joined_by_a_blank_line() {
    assert_eq!(given(&Prompt::from_state(&skey("topic"))), "tides");
    assert_eq!(
        given(&Prompt::from_state_list(&skey("rules"))),
        "Be brief.\n\nCite."
    );
    assert_eq!(given(&Prompt::from_config(&ckey("tone"))), "plain");
    assert_eq!(given(&Prompt::from_config_list(&ckey("notes"))), "");
}

#[test]
fn given_a_text_read_from_a_key_when_built_then_it_declares_that_key_with_its_kind() {
    let mut expected = Reads::new();
    expected.read_state(&skey::<Vec<String>>("rules"));
    assert_eq!(
        Prompt::<LlmCall>::from_state_list(&skey("rules")).reads(),
        &expected
    );
    let mut expected = Reads::new();
    expected.read_config(&ckey::<String>("tone"));
    assert_eq!(
        Prompt::<LlmCall>::from_config(&ckey("tone")).reads(),
        &expected
    );
}

#[test]
fn given_a_text_read_from_a_key_of_another_kind_when_given_then_kind_mismatch() {
    let (state, config) = texts();
    let error = Prompt::<LlmCall>::from_state(&skey("count"))
        .give(&state, &config, ())
        .unwrap_err();
    assert!(matches!(
        error.downcast_ref::<GraphError>(),
        Some(GraphError::KindMismatch { key: named, expected: Kind::Str, found: "int" })
            if *named == key("count")
    ));
}

#[test]
fn given_a_fixed_client_when_given_then_that_client() {
    let client: Arc<dyn Client> = Arc::new(mock(Vec::new()));
    let (state, config) = texts();
    let given = Model::<LlmCall>::client(client.clone())
        .give(&state, &config, ())
        .unwrap();
    assert!(Arc::ptr_eq(&given, &client));
}

#[test]
fn given_a_schedule_from_the_start_when_called_then_each_client_in_turn_and_the_last_repeats() {
    let picked = |then: &[&'static str]| -> Vec<&str> {
        (0..5).map(|index| *from_start(&"a", then, index)).collect()
    };
    assert_eq!(picked(&["b", "c"]), ["a", "b", "c", "c", "c"]);
    assert_eq!(picked(&[]), ["a", "a", "a", "a", "a"]);
}

#[test]
fn given_a_schedule_ending_with_a_tail_when_called_then_the_tail_takes_the_last_calls_allowed() {
    let picked = |tail: &[&'static str], allowed: usize| -> Vec<&str> {
        (0..allowed + 2)
            .map(|index| *with_tail(&"p", tail, index, allowed))
            .collect()
    };
    assert_eq!(picked(&["x", "y"], 4), ["p", "p", "x", "y", "y", "y"]);
    assert_eq!(
        picked(&["x", "y", "z"], 2),
        ["y", "z", "z", "z"],
        "a tail longer than the budget: its last clients"
    );
    assert_eq!(picked(&[], 2), ["p", "p", "p", "p"]);
    assert_eq!(picked(&["x"], 0), ["x", "x"], "no call allowed: past it");
}
