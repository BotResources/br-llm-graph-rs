use br_agentic_nodes::{Judgement, StepLimitEnding};

use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

#[tokio::test(start_paused = true)]
async fn given_the_default_ending_when_the_critic_rejects_at_the_round_limit_then_one_last_pass_runs_and_the_critic_is_not_called_after_it()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("d1"),
        Reply::text("d2"),
        Reply::text("d3"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(false, "m2")]);
    let node = rig.pair(one(&generator), one(&critic), 2);

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 3);
    assert_eq!(critic.calls(), 2);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert_eq!(update.iterations, Some(3));
    assert_eq!(update.text, "d3");
    assert!(!update.step_limit);
    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:d1",
            "critic[Completed]:m1",
            "generator[Completed]:d2",
            "critic[Completed]:m2",
            "generator[Completed]:d3"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_ladders_longer_than_the_rounds_when_the_last_pass_runs_then_it_uses_the_generator_entry_after_the_round_limit_and_no_critic_entry()
 {
    let rig = Rig::new();
    let generators = [
        Scripted::new(vec![Reply::text("d1")]),
        Scripted::new(vec![Reply::text("d2")]),
        Scripted::new(vec![Reply::text("d3")]),
        Scripted::new(vec![]),
    ];
    let critics = [
        Scripted::new(vec![Reply::answer(false, "m1")]),
        Scripted::new(vec![Reply::answer(false, "m2")]),
        Scripted::new(vec![]),
    ];
    let node = build(
        &rig,
        rungs(&[
            &generators[0],
            &generators[1],
            &generators[2],
            &generators[3],
        ]),
        rungs(&[&critics[0], &critics[1], &critics[2]]),
        2,
        Setup::default(),
    );

    outcome_of(&node).await;

    let used: Vec<usize> = generators.iter().map(|client| client.calls()).collect();
    assert_eq!(used, vec![1, 1, 1, 0]);
    let judged: Vec<usize> = critics.iter().map(|client| client.calls()).collect();
    assert_eq!(judged, vec![1, 1, 0]);
}

#[tokio::test(start_paused = true)]
async fn given_a_generator_ladder_shorter_than_the_rounds_when_the_last_pass_runs_then_it_uses_the_last_entry()
 {
    let rig = Rig::new();
    let first = Scripted::new(vec![Reply::text("d1")]);
    let last = Scripted::new(vec![
        Reply::text("d2"),
        Reply::text("d3"),
        Reply::text("d4"),
    ]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "m1"),
        Reply::answer(false, "m2"),
        Reply::answer(false, "m3"),
    ]);
    let node = build(
        &rig,
        rungs(&[&first, &last]),
        one(&critic),
        3,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(first.calls(), 1);
    assert_eq!(last.calls(), 3);
    assert_eq!(update.text, "d4");
    assert_eq!(update.iterations, Some(4));
}

#[tokio::test(start_paused = true)]
async fn given_tools_when_the_last_pass_runs_then_it_can_use_them_and_its_steps_count_against_the_step_limit()
 {
    let (tool, probe) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(3);
    let generator = Scripted::new(vec![
        Reply::text("d1"),
        Reply::call("lookup"),
        Reply::text("d2"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(&rig, one(&generator), one(&critic), 1, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert_eq!(generator.calls(), 3);
    assert_eq!(update.text, "d2");
    assert!(!update.step_limit);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
}

#[tokio::test(start_paused = true)]
async fn given_one_step_left_for_the_last_pass_when_it_asks_for_a_tool_then_the_step_limit_stops_it()
 {
    let (tool, probe) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]).with_step_limit(2);
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::call("lookup")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(&rig, one(&generator), one(&critic), 1, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(*probe.starts.lock().unwrap(), 0);
    assert!(update.step_limit);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert_eq!(update.iterations, Some(2));
    assert_eq!(critic.calls(), 1);
    assert_eq!(
        timeline(&rig).last().unwrap(),
        "generator[BudgetExhausted]:call"
    );
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_critic_call_when_the_last_pass_ends_with_a_final_response_then_the_critic_is_not_called_after_it()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let setup = Setup {
        step_ending: StepLimitEnding::WithCriticCall,
        ..Setup::default()
    };
    let node = build(&rig, one(&generator), one(&critic), 1, setup);

    let update = outcome_of(&node).await;

    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert_eq!(update.text, "d2");
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_critic_call_when_the_last_pass_hits_the_step_limit_then_the_critic_is_not_called_and_the_flag_is_set()
 {
    let rig = Rig::new().with_step_limit(2);
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::call("lookup")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let setup = Setup {
        step_ending: StepLimitEnding::WithCriticCall,
        ..Setup::default()
    };
    let node = build(&rig, one(&generator), one(&critic), 1, setup);

    let update = outcome_of(&node).await;

    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert!(update.step_limit);
}

#[tokio::test(start_paused = true)]
async fn given_no_step_left_when_the_critic_rejects_at_the_round_limit_then_it_acts_as_keep_the_rejected_generation()
 {
    let rig = Rig::new().with_step_limit(1);
    let generator = Scripted::new(vec![Reply::text("d1")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(&rig, one(&generator), one(&critic), 1, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::Rejected));
    assert_eq!(update.iterations, Some(1));
    assert!(!update.step_limit);
    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:d1",
            "critic[Completed]:m1"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_item_arriving_during_the_last_pass_when_it_ends_then_the_queue_is_not_drained() {
    let rig = Rig::new().with_inbox();
    let (id, deliver) = human_item(&rig, "late");
    let first = Scripted::new(vec![Reply::text("d1")]);
    let last = Scripted::new(vec![Reply::text("d2")]);
    deliver_on(&last, 1, deliver);
    let generators = ladder(vec![list(&[&first]), list_yielding(&last)]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(&rig, generators, one(&critic), 1, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert!(!rig.inbox.is_read(rig.generator, id));
    assert!(!timeline(&rig).iter().any(|line| line.contains("late")));
}

#[tokio::test(start_paused = true)]
async fn given_a_request_item_drained_after_a_tool_run_of_the_last_pass_when_the_pass_ends_then_it_is_still_the_last_pass()
 {
    let (tool, _) = echo("lookup");
    let rig = Rig::new().with_inbox().with_tools(vec![tool]);
    let (id, deliver) = human_item(&rig, "late");
    let first = Scripted::new(vec![Reply::text("d1")]);
    let last = Scripted::new(vec![Reply::call("lookup"), Reply::text("d2")]);
    deliver_on(&last, 1, deliver);
    let generators = ladder(vec![list(&[&first]), list_yielding(&last)]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(&rig, generators, one(&critic), 1, Setup::default());

    let update = outcome_of(&node).await;

    assert!(rig.inbox.is_read(rig.generator, id));
    assert!(timeline(&rig).iter().any(|line| line.contains("in:late")));
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert_eq!(update.iterations, Some(2));
    assert_eq!(update.text, "d2");
}
