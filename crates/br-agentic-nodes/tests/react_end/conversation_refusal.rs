use std::sync::Arc;

use br_graph_contract::Event;
use br_llm_messages::{Conversation, ConversationId, HumanMessage, InputTurn, Turn};

use crate::access_spy::{AccessSpy, Op};
use crate::clients::ReusedId;
use crate::common::*;
use crate::kit::{Parts, react_list};

#[tokio::test(start_paused = true)]
async fn given_a_response_the_conversation_refuses_when_the_agent_runs_then_the_thread_access_gets_no_completion_and_the_instance_fails()
 {
    let (rig, reused) = rig_with_known_entry();
    let spy = Arc::new(AccessSpy::over(rig.access.clone()));
    let model = ReusedId::of(reused);
    let node = react_with_client(
        &rig,
        Parts::of(&rig).with_access(spy.clone()),
        model.clone(),
    );

    let cause = Driver::failure(&node).await;

    assert_eq!(model.calls(), 1);
    assert_eq!(spy.count(Op::CompleteResponse), 0);
    assert!(ai_turns(&rig.conversation()).is_empty());
    assert!(cause.contains("the rules of br-llm-messages refuse the change"));
}

#[tokio::test(start_paused = true)]
async fn given_a_response_the_conversation_refuses_and_a_thread_access_that_keeps_nothing_when_the_agent_runs_then_the_instance_still_fails()
 {
    let (rig, reused) = rig_with_known_entry();
    let spy = Arc::new(AccessSpy::forgetful(rig.conversation(), rig.access.clone()));
    let model = ReusedId::of(reused);
    let node = react_with_client(&rig, Parts::of(&rig).with_access(spy.clone()), model);

    let event = Driver::run(&node).await.1;

    assert!(matches!(event, Event::Failed(_)), "{event:?}");
    assert_eq!(spy.count(Op::CompleteResponse), 0);
}

fn rig_with_known_entry() -> (Rig, br_llm_messages::EntryId) {
    let id = ConversationId::from_uuid(uuid()).unwrap();
    let known = entry_id();
    let message = HumanMessage::new(known, human_id(), vec![text_block("start")]).unwrap();
    let mut conversation = Conversation::new(id);
    let turn = InputTurn::new(turn_id(), vec![message.into()]).unwrap();
    conversation.push_turn(Turn::from(turn)).unwrap();
    (Rig::with_conversation(id, conversation), known)
}

fn react_with_client(
    rig: &Rig,
    parts: Parts,
    model: Arc<ReusedId>,
) -> br_agentic_nodes::ReactAgent<Doc> {
    let wrapped = br_agentic_nodes::ModelClient::new(
        model,
        br_agentic_nodes::ThinkingExpectation::NotExpected,
    );
    react_list(
        rig,
        parts,
        br_agentic_nodes::FallbackList::new(vec![wrapped]).unwrap(),
    )
}
