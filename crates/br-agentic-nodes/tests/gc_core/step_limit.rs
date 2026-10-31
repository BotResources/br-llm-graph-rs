use br_agentic_contract::FileSystemMode;
use br_agentic_nodes::{Judgement, StepLimitEnding};

use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

fn with_critic_call() -> Setup {
    Setup {
        step_ending: StepLimitEnding::WithCriticCall,
        ..Setup::default()
    }
}

#[tokio::test(start_paused = true)]
async fn given_the_default_ending_when_a_pass_is_stopped_by_the_step_limit_then_the_critic_is_not_called_and_the_flag_is_set()
 {
    let (tool, probe) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(2);
    let generator = Scripted::new(vec![Reply::call("lookup"), Reply::call("lookup")]);
    let critic = Scripted::new(vec![]);
    let node = rig.pair(one(&generator), one(&critic), 3);

    let update = outcome_of(&node).await;

    assert_eq!(critic.calls(), 0);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert!(update.step_limit);
    assert_eq!(update.iterations, Some(1));
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert_eq!(turns_of(&rig, "critic"), 0);
    let outcome = rig.outcome.pair_outcomes().pop().unwrap();
    assert!(outcome.response().runtime_calls().next().is_some());
    let conversation = rig.conversation();
    let last = ai_turns(&conversation).pop().unwrap();
    let reason = format!("{:?}", closure(last).unwrap());
    assert!(reason.starts_with("BudgetExhausted"), "{reason}");
    assert!(reason.contains("step limit"), "{reason}");
}

#[tokio::test(start_paused = true)]
async fn given_a_pass_stopped_by_the_step_limit_when_an_item_is_waiting_then_no_drain_and_no_new_pass_happen()
 {
    let rig = Rig::new().with_inbox().with_step_limit(1);
    let (id, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::call("lookup")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        3,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert!(update.step_limit);
    assert_eq!(generator.calls(), 1);
    assert!(!rig.inbox.is_read(rig.generator, id));
    assert!(!timeline(&rig).contains(&"in:later".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_critic_call_when_the_critic_validates_at_the_step_limit_then_the_judgement_is_validated()
 {
    let rig = Rig::new().with_step_limit(1);
    let generator = Scripted::new(vec![Reply::call("lookup")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, with_critic_call());

    let update = outcome_of(&node).await;

    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(update.step_limit);
    assert_eq!(update.iterations, Some(1));
    assert_eq!(turns_of(&rig, "critic"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_critic_call_when_the_critic_validates_at_the_step_limit_then_the_queue_is_not_drained()
 {
    let rig = Rig::new().with_inbox().with_step_limit(1);
    let (id, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::call("lookup")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        3,
        with_critic_call(),
    );

    outcome_of(&node).await;

    assert_eq!(generator.calls(), 1);
    assert!(!rig.inbox.is_read(rig.generator, id));
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_critic_call_when_the_critic_rejects_at_the_step_limit_then_the_instance_fails_and_stores_nothing_of_the_call()
 {
    let rig = Rig::new()
        .with_step_limit(1)
        .with_files(FileSystemMode::Always, Vec::new());
    let generator = Scripted::new(vec![Reply::call("lookup")]);
    let critic = Scripted::new(vec![Reply::answer(false, "Not finished at all")]);
    let node = build(&rig, one(&generator), one(&critic), 3, with_critic_call());

    let reason = failed(&node).await;

    assert!(reason.contains("step limit"), "{reason}");
    assert!(reason.contains("Not finished at all"), "{reason}");
    assert_eq!(turns_of(&rig, "critic"), 0);
    assert_eq!(rig.outcome.calls(), 0);
    assert_eq!(count(&rig, "workspace_save"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_at_the_last_step_when_the_pass_ends_then_it_ends_normally_and_the_critic_is_called_with_either_ending()
 {
    for setup in [Setup::default(), with_critic_call()] {
        let (tool, _) = echo("lookup");
        let rig = Rig::new().with_tools(vec![tool]).with_step_limit(2);
        let generator = Scripted::new(vec![Reply::call("lookup"), Reply::text("done")]);
        let critic = Scripted::new(vec![Reply::answer(true, "")]);
        let node = build(&rig, one(&generator), one(&critic), 3, setup);

        let update = outcome_of(&node).await;

        assert_eq!(critic.calls(), 1);
        assert_eq!(update.judgement, Some(Judgement::Validated));
        assert!(!update.step_limit);
        assert_eq!(update.text, "done");
    }
}

#[tokio::test(start_paused = true)]
async fn given_steps_over_two_passes_when_the_second_pass_makes_the_last_step_then_the_step_counter_spans_the_passes()
 {
    let (tool, probe) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(3);
    let generator = Scripted::new(vec![
        Reply::call("lookup"),
        Reply::text("d1"),
        Reply::call("lookup"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = rig.pair(one(&generator), one(&critic), 5);

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 3);
    assert_eq!(critic.calls(), 1);
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert!(update.step_limit);
    assert_eq!(update.iterations, Some(2));
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_critic_call_when_a_second_round_hits_the_step_limit_then_the_critic_is_called_for_that_round_with_its_ladder_entry()
 {
    let rig = Rig::new().with_step_limit(2);
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::call("lookup")]);
    let c1 = Scripted::new(vec![Reply::answer(false, "m1")]);
    let c2 = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(
        &rig,
        one(&generator),
        rungs(&[&c1, &c2]),
        5,
        with_critic_call(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(calls_of(&[&c1, &c2]), vec![1, 1]);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert!(update.step_limit);
    assert_eq!(update.iterations, Some(2));
}
