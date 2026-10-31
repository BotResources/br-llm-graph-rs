use br_agentic_contract::FileSystemMode;
use br_agentic_nodes::{Judgement, RoundLimitEnding};

use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

fn setup(ending: RoundLimitEnding) -> Setup {
    Setup {
        round_ending: ending,
        ..Setup::default()
    }
}

#[tokio::test(start_paused = true)]
async fn given_keep_the_rejected_generation_when_the_critic_rejects_at_the_round_limit_then_the_instance_ends_with_the_rejected_generation()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(false, "m2")]);
    let node = build(
        &rig,
        one(&generator),
        one(&critic),
        2,
        setup(RoundLimitEnding::KeepRejectedGeneration),
    );

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 2);
    assert_eq!(critic.calls(), 2);
    assert_eq!(update.judgement, Some(Judgement::Rejected));
    assert_eq!(update.text, "d2");
    assert_eq!(update.iterations, Some(2));
    assert!(!update.step_limit);
    assert_eq!(timeline(&rig).last().unwrap(), "critic[Completed]:m2");
}

#[tokio::test(start_paused = true)]
async fn given_keep_the_rejected_generation_when_an_item_is_waiting_at_the_round_limit_then_the_queue_is_not_drained()
 {
    let rig = Rig::new().with_inbox();
    let (id, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::text("d1")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        1,
        setup(RoundLimitEnding::KeepRejectedGeneration),
    );

    let update = outcome_of(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Rejected));
    assert!(!rig.inbox.is_read(rig.generator, id));
    assert!(!timeline(&rig).contains(&"in:later".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn given_fail_when_the_critic_rejects_at_the_round_limit_then_the_instance_fails_with_the_message_and_stores_nothing_of_the_call()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let generator = Scripted::new(vec![Reply::text("d1")]);
    let critic = Scripted::new(vec![Reply::answer(false, "Too long by 3 characters")]);
    let node = build(
        &rig,
        one(&generator),
        one(&critic),
        1,
        setup(RoundLimitEnding::Fail),
    );

    let reason = failed(&node).await;

    assert!(reason.contains("round limit"), "{reason}");
    assert!(reason.contains("Too long by 3 characters"), "{reason}");
    assert_eq!(turns_of(&rig, "critic"), 0);
    assert_eq!(timeline(&rig), vec!["in:start", "generator[Completed]:d1"]);
    assert_eq!(rig.outcome.calls(), 0);
    assert_eq!(count(&rig, "workspace_save"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_fail_when_the_limit_is_two_then_the_first_message_is_stored_and_only_the_second_call_is_dropped()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(false, "m2")]);
    let node = build(
        &rig,
        one(&generator),
        one(&critic),
        2,
        setup(RoundLimitEnding::Fail),
    );

    let reason = failed(&node).await;

    assert!(reason.contains("m2"), "{reason}");
    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:d1",
            "critic[Completed]:m1",
            "generator[Completed]:d2"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_fail_when_the_critic_validates_before_the_limit_then_the_instance_ends_normally() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]);
    let node = build(
        &rig,
        one(&generator),
        one(&critic),
        2,
        setup(RoundLimitEnding::Fail),
    );

    let update = outcome_of(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
}

#[tokio::test(start_paused = true)]
async fn given_fail_and_no_step_left_when_the_critic_rejects_at_the_round_limit_then_the_instance_still_fails()
 {
    let rig = Rig::new().with_step_limit(1);
    let generator = Scripted::new(vec![Reply::text("d1")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(
        &rig,
        one(&generator),
        one(&critic),
        1,
        setup(RoundLimitEnding::Fail),
    );

    let reason = failed(&node).await;

    assert!(reason.contains("round limit"), "{reason}");
    assert_eq!(turns_of(&rig, "critic"), 0);
}
