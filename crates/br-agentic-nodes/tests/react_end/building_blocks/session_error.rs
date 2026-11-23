use br_agentic_contract::{AiTurnRef, ApplyError, apply_closure};
use br_agentic_nodes::SessionError;
use br_llm_messages::{ClosureReason, Conversation, TurnClosure};

use crate::building_blocks::common::*;

#[test]
fn given_an_apply_error_of_a_wrong_turn_when_it_converts_then_it_is_a_wrong_turn_error_of_the_same_turn()
 {
    let turn = AiTurnRef {
        id: turn_id(),
        agent: agent_id(),
        author: author("agent"),
    };
    let mut conversation = Conversation::new(conversation_id());
    let closure = TurnClosure::new(entry_id(), ClosureReason::completed());
    let refused = apply_closure(&mut conversation, &turn, closure).unwrap_err();
    assert!(matches!(refused, ApplyError::WrongTurn(_)));

    let converted = SessionError::from(refused);

    assert!(matches!(converted, SessionError::WrongTurn(id) if id == turn.id));
}

#[test]
fn given_an_apply_error_of_the_rules_when_it_converts_then_it_is_a_rules_error_with_the_same_source()
 {
    let source = br_llm_messages::PlainText::new("   ").unwrap_err();
    let text = source.to_string();

    let converted = SessionError::from(ApplyError::from(source));

    assert!(matches!(converted, SessionError::Rules(error) if error.to_string() == text));
}

#[test]
fn given_an_error_of_br_llm_messages_when_it_converts_then_it_is_a_rules_error() {
    let refused = br_llm_messages::PlainText::new("   ").unwrap_err();

    let converted = SessionError::from(refused);

    assert!(matches!(converted, SessionError::Rules(_)));
}

#[test]
fn given_a_port_error_when_a_session_error_wraps_it_then_the_text_is_the_one_of_the_port_error() {
    let error = SessionError::Port(br_agentic_contract::PortError::fault("boom"));

    assert_eq!(error.to_string(), "a port failed");
}
