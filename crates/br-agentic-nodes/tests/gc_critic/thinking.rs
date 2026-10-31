use crate::pair::Pair;
use crate::prelude::*;

#[tokio::test(start_paused = true)]
async fn given_a_client_expected_to_think_when_the_critic_answers_without_thinking_block_then_the_next_client_is_called()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let silent = Scripted::new(vec![Reply::answer(true, "")]);
    let next = Scripted::new(vec![Reply::answer(true, "")]);
    let entry = FallbackList::new(vec![thinking_client(&silent), client(&next)]).unwrap();
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!((silent.calls(), next.calls()), (1, 1));
}

#[tokio::test(start_paused = true)]
async fn given_a_client_expected_to_think_when_the_critic_answers_without_thinking_block_and_no_client_is_left_then_the_instance_fails()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let silent = Scripted::new(vec![Reply::answer(true, "")]);
    let entry = FallbackList::from(thinking_client(&silent));
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("think"), "{cause}");
    assert_eq!(silent.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_client_expected_to_think_when_the_critic_answers_with_a_thinking_block_then_the_answer_is_used()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let thoughtful = Scripted::new(vec![Reply::thinking(Reply::answer(true, ""))]);
    let entry = FallbackList::from(thinking_client(&thoughtful));
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
}

#[tokio::test(start_paused = true)]
async fn given_a_client_not_expected_to_think_when_the_critic_answers_with_a_thinking_block_then_the_answer_is_used()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::thinking(Reply::answer(true, ""))]);
    let node = Pair::new(&rig, &generator, &critic).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
}

#[tokio::test(start_paused = true)]
async fn given_a_second_round_when_the_critic_client_answers_without_thinking_block_then_the_next_client_is_called()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let moody = Scripted::new(vec![
        Reply::thinking(Reply::answer(false, "improve")),
        Reply::answer(true, ""),
    ]);
    let next = Scripted::new(vec![Reply::answer(true, "")]);
    let entry = FallbackList::new(vec![thinking_client(&moody), client(&next)]).unwrap();
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!((moody.calls(), next.calls()), (2, 1));
}

#[tokio::test(start_paused = true)]
async fn given_a_second_round_when_the_only_critic_client_answers_without_thinking_block_then_the_instance_fails()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let moody = Scripted::new(vec![
        Reply::thinking(Reply::answer(false, "improve")),
        Reply::answer(true, ""),
    ]);
    let entry = FallbackList::from(thinking_client(&moody));
    let node = Pair::ladders(&rig, one(&generator), ladder(vec![entry])).build();

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("think"), "{cause}");
    assert_eq!(moody.calls(), 2);
}
