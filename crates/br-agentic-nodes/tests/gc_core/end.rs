use br_agentic_contract::FileSystemMode;
use br_agentic_nodes::{Judgement, RoundLimitEnding};

use crate::common::*;
use crate::kit::*;
use crate::view::*;

#[tokio::test(start_paused = true)]
async fn given_a_kept_rejection_when_the_instance_ends_then_the_outcome_holds_the_response_of_the_generator_and_not_the_message_of_the_critic()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("the work")]);
    let critic = Scripted::new(vec![Reply::answer(false, "the critique")]);
    let setup = Setup {
        round_ending: RoundLimitEnding::KeepRejectedGeneration,
        ..Setup::default()
    };
    let node = build(&rig, one(&generator), one(&critic), 1, setup);

    let update = outcome_of(&node).await;

    let outcomes = rig.outcome.pair_outcomes();
    assert_eq!(outcomes.len(), 1);
    assert_eq!(text_of(outcomes[0].response()), "the work");
    assert_eq!(outcomes[0].judgement(), Judgement::Rejected);
    assert_eq!(outcomes[0].iterations(), 1);
    assert!(!outcomes[0].step_limit_reached());
    assert_eq!(update.text, "the work");
    assert_eq!(update.judgement, Some(Judgement::Rejected));
    assert_eq!(update.iterations, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_files_mode_when_the_instance_ends_with_a_result_then_the_files_are_saved_once() {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(count(&rig, "workspace_load"), 1);
    assert_eq!(count(&rig, "workspace_save"), 1);
    assert_eq!(rig.outcome.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_files_mode_when_the_outcome_port_fails_then_the_instance_fails_and_the_files_are_not_saved()
 {
    let rig = Rig::new()
        .with_files(FileSystemMode::Always, Vec::new())
        .with_outcome(Recording::failing());
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let reason = failed(&node).await;

    assert!(reason.contains("outcome port failed"), "{reason}");
    assert_eq!(count(&rig, "workspace_save"), 0);
}
