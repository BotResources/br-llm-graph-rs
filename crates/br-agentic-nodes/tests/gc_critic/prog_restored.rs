use std::sync::Arc;

use br_agentic_contract::memory::MemoryThreadAccess;
use br_llm_messages::{
    AITurn, ContentBlock, Conversation, ModelResponse, ResponseMetadata, TextBlock, Turn,
    TurnClosure, TurnEntry,
};

use crate::pair::Pair;
use crate::ports::{Checker, Verdict, refuse};
use crate::prelude::*;

fn restored(rig: &mut Rig, text: &str, reason: ClosureReason) {
    let mut conversation: Conversation = conversation_with(rig.conversation_id, "start");
    let mut turn = AITurn::open(turn_id(), rig.generator, author("Generator"));
    let response = ModelResponse::new(
        entry_id(),
        vec![ContentBlock::Text(
            TextBlock::new(block_id(), text).unwrap(),
        )],
        ResponseEnding::finished(),
        ResponseMetadata::new(),
    )
    .unwrap();
    turn.push_entry(TurnEntry::ModelResponse(response)).unwrap();
    turn.push_entry(TurnEntry::TurnClosure(TurnClosure::new(entry_id(), reason)))
        .unwrap();
    conversation.push_turn(Turn::from(turn)).unwrap();
    rig.access = Arc::new(MemoryThreadAccess::new(
        &rig.record,
        rig.thread,
        conversation,
    ));
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_when_the_instance_starts_then_the_programmatic_critic_checks_it_and_no_pass_runs()
 {
    let mut rig = Rig::new();
    restored(&mut rig, "restored draft", ClosureReason::completed());
    let generator = Scripted::new(vec![Reply::text("never")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(generator.calls(), 0);
    assert_eq!(checker.texts_seen(), vec!["restored draft".to_owned()]);
    assert_eq!(checker.refusals_seen(), vec![0]);
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.iterations, Some(0));
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_when_the_programmatic_critic_refuses_it_then_a_pass_repairs_it()
 {
    let mut rig = Rig::new();
    restored(&mut rig, "restored draft", ClosureReason::completed());
    let generator = Scripted::new(vec![Reply::text("repaired")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse("fix the draft"), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(generator.calls(), 1);
    assert_eq!(
        checker.texts_seen(),
        vec!["restored draft".to_owned(), "repaired".to_owned()]
    );
    assert_eq!(checker.refusals_seen(), vec![0, 1]);
    assert_eq!(update.text, "repaired");
    assert_eq!(update.iterations, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_response_stopped_by_the_step_limit_when_the_instance_starts_then_the_programmatic_critic_is_not_called()
 {
    let mut rig = Rig::new();
    restored(&mut rig, "cut short", ClosureReason::budget_exhausted());
    let generator = Scripted::new(vec![Reply::text("never")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse("never asked")]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 1)
        .build();

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert!(checker.checked().is_empty());
    assert_eq!(generator.calls(), 0);
    assert_eq!(critic.calls(), 0);
}
