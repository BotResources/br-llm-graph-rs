//! Which replies cannot be used, why, the reminder that asks for one again
//! and where it goes in the request.

use std::collections::BTreeMap;

use br_llm_clients_core::{
    ClientError, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_messages::{StopReason, Text, WireMessage, WireUserBlock};
use serde_json::json;

use super::{REMINDER, Unusable, judged, reminded, reminder, unfit};
use crate::error::GraphError;
use crate::react::given::{CallFacts, Prompt, Reads};
use crate::react::llm_call::LlmCall;
use crate::react::test_support::{key, skey, structured_step, text_step, tool_call_step};
use crate::state::{Config, Kind, Schema, State, Value};

fn answer_shape() -> OutputMode {
    let parameters = Parameters::new(vec![
        Parameter::new("answer", "", true, ParameterKind::String).unwrap(),
    ])
    .unwrap();
    OutputMode::Structured(StructuredShape::new("reply", None, parameters).unwrap())
}

fn text(body: &str) -> Text {
    Text::new(body).unwrap()
}

#[test]
fn given_the_plain_mode_when_any_step_is_judged_then_it_can_be_used() {
    for step in [text_step("hi"), structured_step(json!({ "x": 1 }))] {
        assert_eq!(unfit(&step, &OutputMode::Plain), None);
    }
}

#[test]
fn given_a_structured_mode_when_the_final_reply_fits_or_not_then_it_is_used_or_unfit() {
    let mode = answer_shape();
    assert_eq!(
        unfit(&structured_step(json!({ "answer": "tides" })), &mode),
        None
    );
    assert_eq!(
        unfit(&structured_step(json!({ "answer": 3 })), &mode).as_deref(),
        Some("`answer` is not a string")
    );
    assert_eq!(
        unfit(&text_step("tides"), &mode).as_deref(),
        Some("the step carries no structured block")
    );
}

#[test]
fn given_a_structured_mode_when_the_reply_calls_tools_then_it_is_not_checked() {
    let step = tool_call_step("c1", "echo", json!({}));
    assert_eq!(unfit(&step, &answer_shape()), None);
}

#[test]
fn given_a_client_error_when_judged_then_only_unusable_and_malformed_are_replies_to_ask_again() {
    let unusable = ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    };
    let malformed = ClientError::MalformedResponse {
        detail: "not JSON".to_owned(),
    };
    for error in [unusable, malformed] {
        let judged = judged(Err(GraphError::Model(error.clone())), &OutputMode::Plain).unwrap();
        assert_eq!(judged, Err(Unusable::Refused(error)));
    }
    let other = judged(
        Err(GraphError::Model(ClientError::Authentication)),
        &OutputMode::Plain,
    );
    assert!(matches!(
        other,
        Err(GraphError::Model(ClientError::Authentication))
    ));
}

#[test]
fn given_an_unusable_reply_when_no_retry_is_left_then_a_refusal_is_model_and_a_misfit_structured() {
    let refused = Unusable::Refused(ClientError::ReplyUnusable {
        stop_reason: StopReason::EndTurn,
    });
    assert_eq!(
        refused.reason(),
        "the reply holds nothing usable (stop reason end_turn)"
    );
    assert!(matches!(
        refused.into_error(),
        GraphError::Model(ClientError::ReplyUnusable { .. })
    ));
    let unfit = Unusable::Unfit("`answer` is missing".to_owned());
    assert!(matches!(
        unfit.into_error(),
        GraphError::Structured { message } if message == "`answer` is missing"
    ));
}

/// A state holding the string key `tone`, and an empty configuration.
fn toned() -> (State, Config) {
    let schema = Schema::builder()
        .state_by_name(key("tone"), Kind::Str)
        .build();
    let state = State::new(
        schema.clone(),
        BTreeMap::from([(key("tone"), Value::str("Be brief"))]),
    )
    .unwrap();
    (state, Config::new(&schema, BTreeMap::new()).unwrap())
}

#[test]
fn given_no_reminder_when_one_is_asked_then_the_default_holds_the_reason() {
    let (state, config) = toned();
    let facts = CallFacts { attempt: 2 };
    let text = reminder::<LlmCall>(None, &state, &config, facts, "`answer` is missing")
        .unwrap()
        .unwrap();
    assert_eq!(
        text.as_str(),
        "Your reply could not be used: `answer` is missing. Reply again."
    );
    assert_eq!(REMINDER.matches("{reason}").count(), 1);
}

#[test]
fn given_a_reminder_function_when_one_is_asked_then_each_reason_placeholder_is_replaced() {
    let (state, config) = toned();
    let mut reads = Reads::new();
    let tone = reads.state::<String>("tone").unwrap();
    let given: Prompt<LlmCall> = Prompt::from_fn(reads, move |ctx| {
        Ok(format!(
            "{}: {{reason}} (attempt {}). Again: {{reason}}.",
            ctx.get(&tone)?,
            ctx.attempt()
        ))
    });
    let text = reminder(
        Some(&given),
        &state,
        &config,
        CallFacts { attempt: 3 },
        "no block",
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        text.as_str(),
        "Be brief: no block (attempt 3). Again: no block."
    );
    let fixed: Prompt<LlmCall> = Prompt::from_state(&skey("tone"));
    let text = reminder(Some(&fixed), &state, &config, CallFacts { attempt: 2 }, "x")
        .unwrap()
        .unwrap();
    assert_eq!(text.as_str(), "Be brief");
}

#[test]
fn given_a_reminder_that_resolves_blank_when_one_is_asked_then_there_is_none_and_the_request_goes_alone()
 {
    let (state, config) = toned();
    let blank: Prompt<LlmCall> = Prompt::text("  ");
    let none = reminder(Some(&blank), &state, &config, CallFacts { attempt: 2 }, "x").unwrap();
    assert_eq!(none, None);
    let user = WireMessage::User {
        content: vec![WireUserBlock::Text { text: text("q") }],
    };
    assert_eq!(reminded(vec![user.clone()], none), [user]);
}

#[test]
fn given_messages_ending_with_a_user_or_assistant_message_when_reminded_then_a_user_message_follows()
 {
    let user = WireMessage::User {
        content: vec![WireUserBlock::Text { text: text("q") }],
    };
    let reminded = reminded(vec![user.clone()], Some(text("again")));
    assert_eq!(
        reminded,
        [
            user,
            WireMessage::User {
                content: vec![WireUserBlock::Text {
                    text: text("again")
                }],
            },
        ]
    );
}

#[test]
fn given_messages_ending_with_a_relay_when_reminded_then_the_relay_takes_the_reminder_last() {
    let user = WireMessage::User {
        content: vec![WireUserBlock::Text { text: text("q") }],
    };
    let relay = WireMessage::Relay {
        content: vec![text("meanwhile")],
    };
    let reminded = reminded(vec![user.clone(), relay], Some(text("again")));
    assert_eq!(
        reminded,
        [
            user,
            WireMessage::Relay {
                content: vec![text("meanwhile"), text("again")],
            },
        ]
    );
}
