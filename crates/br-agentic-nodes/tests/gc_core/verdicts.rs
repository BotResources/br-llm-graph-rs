use br_agentic_nodes::{Judgement, StepLimitEnding};
use br_llm_messages::ResponseEnding;

use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

const MESSAGE: &str = "Fix the title.\nMake it shorter, please.";

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_validates_when_the_pair_runs_then_nothing_of_the_call_is_stored_and_the_judgement_is_validated()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "praise that is stored nowhere")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(
        timeline(&rig),
        vec!["in:start", "generator[Completed]:draft"]
    );
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.text, "draft");
    assert_eq!(update.iterations, Some(1));
    assert!(!update.step_limit);
    assert_eq!(rig.outcome.pair_outcomes().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_does_not_validate_when_the_pair_runs_then_the_message_is_stored_whole_as_an_ai_turn_of_the_critic()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![Reply::answer(false, MESSAGE), Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    let conversation = rig.conversation();
    let turns: Vec<_> = ai_turns(&conversation)
        .into_iter()
        .filter(|turn| turn.agent() == rig.critic)
        .collect();
    assert_eq!(turns.len(), 1);
    let turn = turns[0];
    assert_eq!(turn.author().as_str(), "Critic");
    assert_eq!(responses(turn), vec![MESSAGE.to_owned()]);
    assert!(matches!(
        closure(turn),
        Some(br_llm_messages::ClosureReason::Completed { .. })
    ));
    let response = turn
        .entries()
        .iter()
        .find_map(|entry| match entry {
            br_llm_messages::TurnEntry::ModelResponse(response) => Some(response),
            _ => None,
        })
        .unwrap();
    assert_eq!(response.blocks().len(), 1);
    assert_eq!(response.ending(), &ResponseEnding::finished());
}

#[tokio::test(start_paused = true)]
async fn given_a_rejection_with_a_step_left_when_the_pair_runs_then_the_generator_passes_again_and_the_judgement_follows_the_last_critic_call()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 2);
    assert_eq!(update.text, "better");
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.iterations, Some(2));
}

#[tokio::test(start_paused = true)]
async fn given_a_rejection_and_no_step_left_when_the_pair_runs_then_the_loop_stops_with_the_judgement_rejected()
 {
    for ending in [
        StepLimitEnding::WithoutCriticCall,
        StepLimitEnding::WithCriticCall,
    ] {
        let rig = Rig::new().with_step_limit(1);
        let generator = Scripted::new(vec![Reply::text("draft")]);
        let critic = Scripted::new(vec![Reply::answer(false, "fix it")]);
        let setup = Setup {
            step_ending: ending,
            ..Setup::default()
        };
        let node = build(&rig, one(&generator), one(&critic), 5, setup);

        let update = outcome_of(&node).await;

        assert_eq!(generator.calls(), 1);
        assert_eq!(critic.calls(), 1);
        assert_eq!(update.judgement, Some(Judgement::Rejected));
        assert_eq!(update.text, "draft");
        assert!(!update.step_limit);
        assert_eq!(update.iterations, Some(1));
        assert_eq!(
            timeline(&rig),
            vec![
                "in:start",
                "generator[Completed]:draft",
                "critic[Completed]:fix it"
            ]
        );
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_rejection_and_no_step_left_when_an_item_is_waiting_then_the_queue_is_not_drained()
{
    let rig = Rig::new().with_inbox().with_step_limit(1);
    let (id, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::text("draft")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![Reply::answer(false, "fix it")]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        5,
        Setup::default(),
    );

    outcome_of(&node).await;

    assert!(!rig.inbox.is_read(rig.generator, id));
    assert!(!timeline(&rig).contains(&"in:later".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn given_a_rejection_and_a_step_left_when_items_are_waiting_then_they_form_one_input_turn_after_the_turn_of_the_critic()
 {
    let rig = Rig::new().with_inbox().with_step_limit(2);
    let (first_id, first) = human_item(&rig, "first");
    let (second_id, second) = human_item(&rig, "second");
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    deliver_on(&generator, 1, move || {
        first();
        second();
    });
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        5,
        Setup::default(),
    );

    outcome_of(&node).await;

    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:draft",
            "critic[Completed]:fix it",
            "in:first",
            "in:second",
            "generator[Completed]:better"
        ]
    );
    let conversation = rig.conversation();
    let last_input = input_turns(&conversation).pop().unwrap();
    assert_eq!(last_input.entries().len(), 2);
    assert!(rig.inbox.is_read(rig.generator, first_id));
    assert!(rig.inbox.is_read(rig.generator, second_id));
    let seen = generator.requests().pop().unwrap();
    assert!(format!("{:?}", seen.history()).contains("second"));
}

#[tokio::test(start_paused = true)]
async fn given_a_validation_and_a_step_left_when_an_item_is_waiting_then_one_more_pass_uses_that_drain_and_stores_the_item_once()
 {
    let rig = Rig::new().with_inbox().with_step_limit(2);
    let (id, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![Reply::answer(true, ""), Reply::answer(true, "")]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        5,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:d1",
            "in:later",
            "generator[Completed]:d2"
        ]
    );
    assert!(rig.inbox.is_read(rig.generator, id));
    assert_eq!(update.iterations, Some(2));
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(count(&rig, "inbox_mark_read"), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_validation_and_no_step_left_when_an_item_is_waiting_then_the_item_stays_in_the_inbox()
 {
    let rig = Rig::new().with_inbox().with_step_limit(1);
    let (id, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::text("d1")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        5,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(generator.calls(), 1);
    assert!(!rig.inbox.is_read(rig.generator, id));
    assert_eq!(timeline(&rig), vec!["in:start", "generator[Completed]:d1"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_validation_and_an_empty_queue_when_a_step_is_left_then_the_instance_ends() {
    let rig = Rig::new().with_inbox();
    let generator = Scripted::new(vec![Reply::text("d1")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        5,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 1);
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.iterations, Some(1));
}

#[tokio::test(start_paused = true)]
async fn given_a_validation_and_a_new_pass_that_hits_the_step_limit_when_the_ending_has_no_critic_call_then_the_judgement_is_not_judged()
 {
    let rig = Rig::new().with_inbox().with_step_limit(2);
    let (_, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::call("lookup")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        5,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert!(update.step_limit);
    assert_eq!(update.iterations, Some(2));
}
