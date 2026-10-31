use std::sync::Arc;

use br_agentic_nodes::warning_text;
use br_llm_messages::{
    AITurn, AgentMail, ClosureReason, ContentBlock, Conversation, ConversationId, IncomingEntry,
    InputTurn, ModelResponse, ResponseEnding, ResponseMetadata, RuntimeNotice, TextBlock, Turn,
    TurnClosure, TurnEntry,
};

use crate::access_spy::{AccessSpy, Op};
use crate::common::*;
use crate::kit::{Parts, react_over};

fn escaped(text: &str) -> String {
    let quoted = format!("{text:?}");
    quoted[1..quoted.len() - 1].to_owned()
}

fn crowded_conversation(id: ConversationId) -> Conversation {
    let mut conversation = Conversation::new(id);
    let mail = AgentMail::new(
        entry_id(),
        agent_id(),
        author("Sender"),
        ConversationId::from_uuid(uuid()).unwrap(),
        vec![text_block("a mail body")],
    )
    .unwrap();
    let notice = RuntimeNotice::new(entry_id(), "a notice text").unwrap();
    let first = InputTurn::new(
        turn_id(),
        vec![
            IncomingEntry::from(human_message("the first request")),
            IncomingEntry::from(mail),
            IncomingEntry::from(notice),
        ],
    )
    .unwrap();
    conversation.push_turn(Turn::from(first)).unwrap();
    let mut other = AITurn::open(turn_id(), agent_id(), author("Other"));
    let said = ModelResponse::new(
        entry_id(),
        vec![ContentBlock::Text(
            TextBlock::new(block_id(), "the other agent said").unwrap(),
        )],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )
    .unwrap();
    other.push_entry(TurnEntry::from(said)).unwrap();
    other
        .push_entry(TurnEntry::from(TurnClosure::new(
            entry_id(),
            ClosureReason::completed(),
        )))
        .unwrap();
    conversation.push_turn(Turn::from(other)).unwrap();
    let last = InputTurn::new(
        turn_id(),
        vec![IncomingEntry::from(human_message("the second request"))],
    )
    .unwrap();
    conversation.push_turn(Turn::from(last)).unwrap();
    conversation
}

#[tokio::test(start_paused = true)]
async fn given_a_conversation_with_mails_notices_and_another_agent_when_the_generator_steps_then_it_gets_the_whole_conversation()
 {
    let id = ConversationId::from_uuid(uuid()).unwrap();
    let rig = Rig::with_conversation(id, crowded_conversation(id));
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let seen = history_text(&model.requests()[0]);
    for needle in [
        "the first request",
        "a mail body",
        "a notice text",
        "the other agent said",
        "the second request",
    ] {
        assert!(seen.contains(needle), "{needle} missing from {seen}");
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_step_near_the_limit_when_the_model_is_called_then_the_warning_goes_to_the_request_only_and_is_never_stored()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]).with_step_limit(12);
    let spy = Arc::new(AccessSpy::over(rig.access.clone()));
    let model = Scripted::new(vec![Reply::call("a"), Reply::call("a"), Reply::text("end")]);
    let node = react_over(&rig, Parts::of(&rig).with_access(spy.clone()), &[&model]);
    let warning = escaped(&warning_text(10).unwrap());

    Driver::result(&node).await;

    let requests = model.requests();
    assert!(!history_text(&requests[0]).contains(&warning));
    assert!(!history_text(&requests[1]).contains(&warning));
    assert!(history_text(&requests[2]).contains(&warning));
    assert!(!format!("{:?}", rig.conversation()).contains(&warning));
    assert_eq!(spy.count(Op::AddEntry), 0);
    assert_eq!(spy.count(Op::AddInputTurn), 0);
}

#[tokio::test(start_paused = true)]
async fn given_one_step_left_when_the_model_is_called_then_the_text_that_asks_for_no_tool_goes_to_the_request_only()
 {
    let (tool_a, _) = echo("a");
    let rig = Rig::new().with_tools(vec![tool_a]).with_step_limit(3);
    let model = Scripted::new(vec![Reply::call("a"), Reply::call("a"), Reply::text("end")]);
    let node = rig.react(&[&model]);
    let warning = escaped(&warning_text(1).unwrap());

    Driver::result(&node).await;

    assert!(history_text(&model.requests()[2]).contains(&warning));
    assert!(!format!("{:?}", rig.conversation()).contains(&warning));
    assert!(!format!("{:?}", rig.record.calls()).contains(&warning));
}
