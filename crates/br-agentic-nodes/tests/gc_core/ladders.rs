use br_llm_clients_core::ClientError;

use crate::common::*;
use crate::kit::*;

#[tokio::test(start_paused = true)]
async fn given_a_fallback_list_in_each_ladder_entry_when_a_new_pass_and_a_new_round_start_then_both_lists_are_tried_from_their_first_client()
 {
    let rig = Rig::new();
    let broken_generator = Scripted::new(vec![
        Reply::Fail(ClientError::Transport),
        Reply::Fail(ClientError::Transport),
    ]);
    let working_generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    let broken_critic = Scripted::new(vec![
        Reply::Fail(ClientError::Transport),
        Reply::Fail(ClientError::Transport),
    ]);
    let working_critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]);
    let generators = ladder(vec![list(&[&broken_generator, &working_generator])]);
    let critics = ladder(vec![list(&[&broken_critic, &working_critic])]);
    let node = build(&rig, generators, critics, 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(broken_generator.calls(), 2);
    assert_eq!(working_generator.calls(), 2);
    assert_eq!(broken_critic.calls(), 2);
    assert_eq!(working_critic.calls(), 2);
    assert_eq!(update.text, "d2");
}

#[tokio::test(start_paused = true)]
async fn given_a_fallback_list_in_the_second_entry_when_round_two_starts_then_that_list_serves_the_round_from_its_first_client()
 {
    let rig = Rig::new();
    let first = Scripted::new(vec![Reply::text("d1")]);
    let broken = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let working = Scripted::new(vec![Reply::text("d2")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]);
    let generators = ladder(vec![list(&[&first]), list(&[&broken, &working])]);
    let node = build(&rig, generators, one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(calls_of(&[&first, &broken, &working]), vec![1, 1, 1]);
}
