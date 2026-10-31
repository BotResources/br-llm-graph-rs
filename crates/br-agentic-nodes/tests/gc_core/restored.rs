use br_agentic_nodes::{Judgement, StepLimitEnding};

use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_when_the_critic_validates_then_no_pass_runs_and_the_response_is_no_iteration()
 {
    let mut rig = Rig::new();
    restore_final(&mut rig, "restored");
    let generator = Scripted::new(vec![]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 0);
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.text, "restored");
    assert_eq!(update.iterations, Some(0));
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(!update.step_limit);
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_when_the_critic_rejects_then_one_pass_runs_on_the_second_ladder_entry()
 {
    let mut rig = Rig::new();
    restore_final(&mut rig, "restored");
    let (first, second) = (
        Scripted::new(vec![]),
        Scripted::new(vec![Reply::text("again")]),
    );
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(
        &rig,
        rungs(&[&first, &second]),
        one(&critic),
        3,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(first.calls(), 0);
    assert_eq!(second.calls(), 1);
    assert_eq!(update.text, "again");
    assert_eq!(update.iterations, Some(1));
    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:restored",
            "critic[Completed]:fix it",
            "generator[Completed]:again"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_open_final_turn_when_the_pair_starts_then_it_is_interrupted_and_still_taken_as_the_end_of_a_pass()
 {
    let mut rig = Rig::new();
    restore(&mut rig, final_response("restored"), None);
    let generator = Scripted::new(vec![]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 0);
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.text, "restored");
    assert_eq!(
        timeline(&rig),
        vec!["in:start", "generator[Interrupted]:restored"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_turn_of_the_critic_as_last_turn_when_the_pair_starts_then_the_first_pass_runs_on_the_first_entry()
 {
    let mut rig = Rig::new();
    restore_after_critic(&mut rig, "restored", "fix it");
    let (first, second) = (
        Scripted::new(vec![Reply::text("again")]),
        Scripted::new(vec![]),
    );
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(
        &rig,
        rungs(&[&first, &second]),
        one(&critic),
        3,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 0);
    assert_eq!(update.iterations, Some(1));
    assert!(
        history_agent_outputs(first.requests()[0].history())
            .contains(&(rig.critic, "fix it".to_owned()))
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_budget_exhausted_turn_when_there_is_no_critic_call_then_nothing_runs_and_the_flag_is_set()
 {
    let mut rig = Rig::new();
    restore_exhausted(&mut rig);
    let generator = Scripted::new(vec![]);
    let critic = Scripted::new(vec![]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 0);
    assert_eq!(critic.calls(), 0);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert!(update.step_limit);
    assert_eq!(update.iterations, Some(0));
    let outcome = rig.outcome.pair_outcomes().pop().unwrap();
    assert!(outcome.response().runtime_calls().next().is_some());
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_budget_exhausted_turn_when_the_ending_calls_the_critic_and_it_validates_then_the_judgement_is_validated_and_the_flag_is_set()
 {
    let mut rig = Rig::new();
    restore_exhausted(&mut rig);
    let generator = Scripted::new(vec![]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let setup = Setup {
        step_ending: StepLimitEnding::WithCriticCall,
        ..Setup::default()
    };
    let node = build(&rig, one(&generator), one(&critic), 3, setup);

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 0);
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(update.step_limit);
    assert_eq!(update.iterations, Some(0));
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_budget_exhausted_turn_when_the_ending_calls_the_critic_and_it_rejects_then_the_instance_fails_naming_the_step_limit()
 {
    let mut rig = Rig::new();
    restore_exhausted(&mut rig);
    let generator = Scripted::new(vec![]);
    let critic = Scripted::new(vec![Reply::answer(false, "still wrong")]);
    let setup = Setup {
        step_ending: StepLimitEnding::WithCriticCall,
        ..Setup::default()
    };
    let node = build(&rig, one(&generator), one(&critic), 3, setup);

    let reason = failed(&node).await;

    assert!(reason.contains("step limit"), "{reason}");
    assert!(reason.contains("still wrong"), "{reason}");
    assert_eq!(generator.calls(), 0);
    assert_eq!(turns_of(&rig, "critic"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_and_a_waiting_item_when_the_pair_starts_then_the_first_pass_runs()
 {
    let mut rig = Rig::new().with_inbox();
    restore_final(&mut rig, "restored");
    let (_, deliver) = human_item(&rig, "more work");
    deliver();
    let generator = Scripted::new(vec![Reply::text("after")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 1);
    assert_eq!(update.text, "after");
    assert_eq!(update.iterations, Some(1));
    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:restored",
            "in:more work",
            "generator[Completed]:after"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_and_a_waiting_notice_when_the_pair_starts_then_the_first_pass_runs()
 {
    let mut rig = Rig::new().with_inbox();
    restore_final(&mut rig, "restored");
    let (_, deliver) = notice_item(&rig, "heads up");
    deliver();
    let generator = Scripted::new(vec![Reply::text("after")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 1);
    assert_eq!(update.iterations, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_budget_exhausted_turn_and_a_waiting_item_when_the_pair_starts_then_the_first_pass_runs_and_the_step_limit_does_not_apply()
 {
    let mut rig = Rig::new().with_inbox();
    restore_exhausted(&mut rig);
    let (_, deliver) = human_item(&rig, "more work");
    deliver();
    let generator = Scripted::new(vec![Reply::text("after")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 1);
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(!update.step_limit);
    assert_eq!(update.iterations, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_restored_final_response_in_a_pair_with_an_inbox_and_no_item_when_the_pair_starts_then_no_pass_runs()
 {
    let mut rig = Rig::new().with_inbox();
    restore_final(&mut rig, "restored");
    let generator = Scripted::new(vec![]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 0);
    assert_eq!(update.iterations, Some(0));
}
