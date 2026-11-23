use br_agentic_contract::{
    AiTurnRef, ApplyError, ThreadAccess, apply_closure, apply_entry, apply_input_turn,
    apply_response, apply_results,
};
use br_llm_messages::{
    AgentId, ClosureReason, Conversation, InputTurn, Turn, TurnClosure, TurnEntry,
};

use crate::common::*;

fn conversation() -> Conversation {
    Conversation::new(conversation_id())
}

fn input(text: &str) -> InputTurn {
    InputTurn::new(turn_id(), vec![human(text).into_entry()]).unwrap()
}

fn ai(agent: AgentId, name: &str) -> AiTurnRef {
    AiTurnRef {
        id: turn_id(),
        agent,
        author: author(name),
    }
}

fn closure() -> TurnClosure {
    TurnClosure::new(entry_id(), ClosureReason::completed())
}

#[test]
fn given_an_input_turn_when_it_is_applied_then_the_conversation_holds_it() {
    let mut held = conversation();

    apply_input_turn(&mut held, &input("hello")).unwrap();

    assert!(matches!(held.turns()[0], Turn::InputTurn(_)));
}

#[test]
fn given_an_input_turn_that_the_conversation_holds_when_it_is_applied_again_then_the_rules_refuse_it_and_the_conversation_is_unchanged()
 {
    let mut held = conversation();
    let turn = input("hello");
    apply_input_turn(&mut held, &turn).unwrap();

    let error = apply_input_turn(&mut held, &turn).unwrap_err();

    assert!(matches!(error, ApplyError::Rules(_)));
    assert_eq!(held.turns().len(), 1);
}

#[test]
fn given_the_first_entry_of_a_new_ai_turn_when_it_is_applied_then_the_turn_is_created_with_its_agent_and_author()
 {
    let mut held = conversation();
    let turn = ai(agent_id(), "agent");

    apply_entry(
        &mut held,
        &turn,
        TurnEntry::from(response_text(entry_id(), "one")),
    )
    .unwrap();

    let created = held.turn(&turn.id).unwrap().as_ai().unwrap();
    assert_eq!(
        (created.agent(), created.author()),
        (turn.agent, &turn.author)
    );
}

#[test]
fn given_an_open_ai_turn_when_a_second_entry_is_applied_then_it_goes_into_the_same_turn() {
    let mut held = conversation();
    let turn = ai(agent_id(), "agent");
    let (first, second) = (entry_id(), entry_id());
    apply_response(&mut held, &turn, response_text(first, "one")).unwrap();

    apply_response(&mut held, &turn, response_text(second, "two")).unwrap();

    assert!(held.entry(&first).is_ok() && held.entry(&second).is_ok());
    assert_eq!(held.turns().len(), 1);
}

#[test]
fn given_a_tool_result_for_a_call_of_the_turn_when_it_is_applied_then_the_conversation_holds_it() {
    let mut held = conversation();
    let turn = ai(agent_id(), "agent");
    let call = block_id();
    apply_response(&mut held, &turn, response_call(entry_id(), call, "lookup")).unwrap();
    let result = entry_id();

    apply_results(
        &mut held,
        &turn,
        tool_results(result, call, "lookup", "done"),
    )
    .unwrap();

    assert!(held.entry(&result).is_ok());
}

#[test]
fn given_a_tool_result_for_a_call_that_does_not_exist_when_it_is_applied_then_the_rules_refuse_it()
{
    let mut held = conversation();
    let turn = ai(agent_id(), "agent");

    let error = apply_results(
        &mut held,
        &turn,
        tool_results(entry_id(), block_id(), "lookup", "done"),
    )
    .unwrap_err();

    assert!(matches!(error, ApplyError::Rules(_)));
    assert!(std::error::Error::source(&error).is_some());
    assert!(held.turns().is_empty());
}

#[test]
fn given_a_turn_id_that_names_an_input_turn_when_an_entry_is_applied_then_it_is_the_wrong_turn() {
    let mut held = conversation();
    let input = input("hello");
    let id = input.id();
    apply_input_turn(&mut held, &input).unwrap();
    let turn = AiTurnRef {
        id,
        agent: agent_id(),
        author: author("agent"),
    };

    let error = apply_entry(
        &mut held,
        &turn,
        TurnEntry::from(response_text(entry_id(), "one")),
    )
    .unwrap_err();

    assert!(matches!(error, ApplyError::WrongTurn(wrong) if wrong == id));
}

#[test]
fn given_an_ai_turn_of_another_agent_or_another_author_when_an_entry_is_applied_then_it_is_the_wrong_turn()
 {
    let mut held = conversation();
    let turn = ai(agent_id(), "agent");
    apply_response(&mut held, &turn, response_text(entry_id(), "one")).unwrap();
    let other_agent = AiTurnRef {
        agent: agent_id(),
        ..turn.clone()
    };
    let other_author = AiTurnRef {
        author: author("other"),
        ..turn.clone()
    };

    for wrong in [other_agent, other_author] {
        let error =
            apply_response(&mut held, &wrong, response_text(entry_id(), "two")).unwrap_err();

        assert!(matches!(error, ApplyError::WrongTurn(id) if id == turn.id));
    }
}

#[test]
fn given_an_open_ai_turn_when_a_closure_is_applied_then_the_turn_is_closed() {
    let mut held = conversation();
    let turn = ai(agent_id(), "agent");
    apply_response(&mut held, &turn, response_text(entry_id(), "one")).unwrap();

    apply_closure(&mut held, &turn, closure()).unwrap();

    assert!(held.turn(&turn.id).unwrap().as_ai().unwrap().is_closed());
}

#[test]
fn given_an_ai_turn_that_does_not_exist_when_a_closure_is_applied_then_it_is_the_wrong_turn_and_no_turn_is_created()
 {
    let mut held = conversation();
    let turn = ai(agent_id(), "agent");

    let error = apply_closure(&mut held, &turn, closure()).unwrap_err();

    assert!(matches!(error, ApplyError::WrongTurn(id) if id == turn.id));
    assert!(held.turns().is_empty());
}

#[tokio::test]
async fn given_a_write_that_the_rules_refuse_when_the_in_memory_thread_access_gets_it_then_it_is_a_fault_and_the_conversation_is_unchanged()
 {
    let world = World::new();
    let turn = input("hello");
    world
        .access
        .add_input_turn(&world.thread, turn.clone())
        .await
        .unwrap();

    let error = world
        .access
        .add_input_turn(&world.thread, turn)
        .await
        .unwrap_err();

    assert!(!error.is_refusal());
    assert_eq!(world.access.conversation().turns().len(), 1);
}
