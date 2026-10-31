use br_graph_contract::Event;

use crate::pair::{Pair, turns_of};
use crate::prelude::*;

fn rendered(rig: &Rig) -> String {
    format!("{:?}{:?}", rig.conversation(), rig.record.calls())
}

#[tokio::test(start_paused = true)]
async fn given_a_validating_answer_without_message_when_the_critic_answers_then_the_answer_is_accepted()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::Structured(
        "{\"is_validated\":true}".to_owned(),
    )]);
    let node = Pair::new(&rig, &generator, &critic).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
}

#[tokio::test(start_paused = true)]
async fn given_a_rejecting_answer_with_a_thinking_field_when_the_critic_answers_then_the_thinking_is_stored_nowhere()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::Structured(
            "{\"thinking\":\"SECRET-REASONING\",\"is_validated\":false,\"message\":\"fix it\"}"
                .to_owned(),
        ),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    assert!(!rendered(&rig).contains("SECRET-REASONING"));
    for request in generator.requests() {
        assert!(!history_text(&request).contains("SECRET-REASONING"));
    }
    let turns = turns_of(&rig, rig.critic);
    assert_eq!(turns.len(), 1);
    assert_eq!(responses(&turns[0]), vec!["fix it".to_owned()]);
}

#[tokio::test(start_paused = true)]
async fn given_a_validating_answer_with_a_thinking_field_when_the_critic_answers_then_it_validates_and_stores_nothing()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::Structured(
        "{\"thinking\":\"SECRET-REASONING\",\"is_validated\":true,\"message\":\"\"}".to_owned(),
    )]);
    let node = Pair::new(&rig, &generator, &critic).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(!rendered(&rig).contains("SECRET-REASONING"));
}

#[tokio::test(start_paused = true)]
async fn given_an_answer_with_other_keys_when_the_critic_answers_then_only_the_two_keys_are_read_and_the_others_are_stored_nowhere()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::Structured(
            "{\"is_validated\":false,\"message\":\"fix it\",\"mood\":\"OTHER-KEY-VALUE\"}"
                .to_owned(),
        ),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic).build();

    let (_driver, event) = Driver::run(&node).await;

    assert!(matches!(event, Event::Result(_)));
    assert!(!rendered(&rig).contains("OTHER-KEY-VALUE"));
}

#[tokio::test(start_paused = true)]
async fn given_a_client_expected_to_think_and_an_answer_without_thinking_field_when_the_critic_answers_then_the_absence_is_no_error()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::thinking(Reply::answer(true, ""))]);
    let list = FallbackList::from(thinking_client(&critic));
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![list])).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
}

#[tokio::test(start_paused = true)]
async fn given_a_client_not_expected_to_think_and_an_answer_without_thinking_field_when_the_critic_answers_then_the_absence_is_no_error()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
}
