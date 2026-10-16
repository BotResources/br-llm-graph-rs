//! How each constructor of a subject says it was given: a fixed value, a
//! ready-made value or a function, as the description writes it.

use std::sync::Arc;

use br_llm_clients_core::Client;

use crate::react::given::{Given, History, Model, Origin, Prompt, Reads};
use crate::react::llm_call::LlmCall;
use crate::react::react_agent::ReactAgent;
use crate::react::seats::GeneratorSeat;
use crate::react::test_support::{ckey, mock, skey};

fn client() -> Arc<dyn Client> {
    Arc::new(mock(Vec::new()))
}

#[test]
fn given_a_value_a_text_or_a_client_when_asked_then_fixed() {
    assert_eq!(
        Prompt::<LlmCall>::value("one".to_owned()).origin(),
        Origin::Fixed
    );
    assert_eq!(Prompt::<LlmCall>::text("one").origin(), Origin::Fixed);
    assert_eq!(Model::<LlmCall>::client(client()).origin(), Origin::Fixed);
    assert_eq!(
        History::<LlmCall>::value(Vec::new()).origin(),
        Origin::Fixed
    );
}

#[test]
fn given_a_text_read_from_a_key_when_asked_then_ready_made() {
    let made = [
        Prompt::<LlmCall>::from_state(&skey("topic")),
        Prompt::<LlmCall>::from_state_list(&skey("rules")),
        Prompt::<LlmCall>::from_config(&ckey("tone")),
        Prompt::<LlmCall>::from_config_list(&ckey("notes")),
    ];
    for prompt in made {
        assert_eq!(prompt.origin(), Origin::ReadyMade);
    }
}

#[test]
fn given_a_schedule_of_clients_when_asked_then_ready_made() {
    assert_eq!(
        Model::<ReactAgent>::schedule(client(), vec![client()]).origin(),
        Origin::ReadyMade
    );
    assert_eq!(
        Model::<GeneratorSeat>::ending_with(client(), vec![client()]).origin(),
        Origin::ReadyMade
    );
}

#[test]
fn given_a_function_when_asked_then_function() {
    let given: Given<LlmCall, String> = Given::from_fn(Reads::new(), |_| Ok(String::new()));
    assert_eq!(given.origin(), Origin::Function);
}

#[test]
fn given_a_subject_when_cloned_then_its_origin_is_kept() {
    assert_eq!(
        Prompt::<LlmCall>::text("one").clone().origin(),
        Origin::Fixed
    );
    assert_eq!(
        Prompt::<LlmCall>::from_state(&skey("topic"))
            .clone()
            .origin(),
        Origin::ReadyMade
    );
    let function: Prompt<LlmCall> = Given::from_fn(Reads::new(), |_| Ok(String::new()));
    assert_eq!(function.clone().origin(), Origin::Function);
}
