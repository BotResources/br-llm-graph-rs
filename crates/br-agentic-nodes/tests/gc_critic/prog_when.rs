use br_agentic_contract::memory::Call;

use crate::pair::{Pair, calls_for, turns_of};
use crate::ports::{Checker, Verdict, log, mark_on, marks};
use crate::prelude::*;

fn agent_labels(rig: &Rig) -> Vec<String> {
    rig.record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::AgentPort { label, .. } => Some(label.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn given_no_programmatic_critic_when_the_instance_starts_then_the_agent_port_is_not_asked_for_it()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = Pair::new(&rig, &generator, &critic).build();

    Driver::result(&node).await;

    let labels = agent_labels(&rig);
    assert!(!labels.contains(&"programmatic_critic".to_owned()));
    assert_eq!(rig.prompts.asked("programmatic_critic"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_programmatic_critic_when_the_instance_starts_then_the_agent_port_is_asked_for_it_and_the_prompt_port_is_not()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    Driver::result(&node).await;

    let labels = agent_labels(&rig);
    assert!(labels.contains(&"programmatic_critic".to_owned()));
    assert!(labels.contains(&"generator".to_owned()));
    assert!(labels.contains(&"critic".to_owned()));
    assert_eq!(rig.prompts.asked("programmatic_critic"), 0);
    assert_eq!(rig.prompts.asked("generator"), 1);
    assert_eq!(rig.prompts.asked("critic"), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_pass_that_ends_with_a_final_response_when_the_round_runs_then_the_programmatic_critic_is_called_before_the_critic()
 {
    let rig = Rig::new();
    let marks_log = log();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    generator.on_call(mark_on(&marks_log, "generator"));
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    critic.on_call(mark_on(&marks_log, "critic"));
    let checker = Checker::logging(vec![Verdict::Accept], &marks_log);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    Driver::result(&node).await;

    assert_eq!(
        marks(&marks_log),
        vec!["generator", "programmatic", "critic"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_pass_when_the_programmatic_critic_is_called_then_it_gets_the_snapshot_the_last_response_and_the_refusal_count()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("the draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    Driver::result(&node).await;

    let checked = checker.checked();
    assert_eq!(checked.len(), 1);
    assert_eq!(checked[0].text, "the draft");
    assert_eq!(checked[0].refusals, 0);
    assert_eq!(checked[0].note, "start");
}

#[tokio::test(start_paused = true)]
async fn given_an_accepting_programmatic_critic_when_the_round_runs_then_nothing_is_stored_and_the_critic_is_called()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(critic.calls(), 1);
    assert!(turns_of(&rig, rig.programmatic).is_empty());
    assert!(calls_for(&rig, rig.programmatic).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_the_last_pass_of_the_round_limit_ending_when_it_ends_then_the_programmatic_critic_is_called_and_the_critic_is_not()
 {
    let rig = Rig::new();
    let marks_log = log();
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    generator.on_call(mark_on(&marks_log, "generator"));
    let critic = Scripted::new(vec![Reply::answer(false, "improve")]);
    critic.on_call(mark_on(&marks_log, "critic"));
    let checker = Checker::logging(vec![Verdict::Accept, Verdict::Accept], &marks_log);
    let node = Pair::new(&rig, &generator, &critic)
        .round_limit(1)
        .round_ending(RoundLimitEnding::FinalGeneration)
        .programmatic(&checker, 2)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert_eq!(
        checker.texts_seen(),
        vec!["one".to_owned(), "two".to_owned()]
    );
    assert_eq!(critic.calls(), 1);
    assert_eq!(
        marks(&marks_log),
        vec![
            "generator",
            "programmatic",
            "critic",
            "generator",
            "programmatic"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_pass_stopped_by_the_step_limit_when_the_instance_ends_then_the_programmatic_critic_is_not_called_and_the_outcome_holds_the_response()
 {
    let rig = Rig::new().with_step_limit(1);
    let generator = Scripted::new(vec![Reply::call("wanted")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Refuse("never asked".to_owned())]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 1)
        .build();

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert!(checker.checked().is_empty());
    assert_eq!(critic.calls(), 0);
    let outcomes = rig.outcome.pair_outcomes();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].response().runtime_calls().next().is_some());
    assert!(turns_of(&rig, rig.programmatic).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_pass_stopped_by_the_step_limit_and_a_critic_call_ending_when_the_instance_ends_then_the_critic_is_called_and_the_programmatic_critic_is_not()
 {
    let rig = Rig::new().with_step_limit(1);
    let generator = Scripted::new(vec![Reply::call("wanted")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Refuse("never asked".to_owned())]);
    let node = Pair::new(&rig, &generator, &critic)
        .step_ending(StepLimitEnding::WithCriticCall)
        .programmatic(&checker, 1)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(critic.calls(), 1);
    assert!(checker.checked().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_an_accepting_programmatic_critic_when_the_outcome_is_built_then_it_holds_the_accepted_response()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("accepted draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 2)
        .build();

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "accepted draft");
    assert!(!update.step_limit);
    assert_eq!(checker.texts_seen(), vec!["accepted draft".to_owned()]);
}
