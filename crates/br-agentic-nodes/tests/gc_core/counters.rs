use br_agentic_nodes::{Judgement, RoundLimitEnding};

use crate::common::*;
use crate::items::*;
use crate::kit::*;

#[derive(Clone, Copy)]
enum Kind {
    Human,
    MailTo,
    MailCc,
    Notice,
}

fn arrival(rig: &Rig, kind: Kind) -> Box<dyn Fn() + Send + Sync> {
    match kind {
        Kind::Human => Box::new(human_item(rig, "item").1),
        Kind::MailTo => Box::new(mail_item(rig, false).1),
        Kind::MailCc => Box::new(mail_item(rig, true).1),
        Kind::Notice => Box::new(notice_item(rig, "item").1),
    }
}

async fn item_during_round_two(kind: Kind) -> (Vec<usize>, Vec<usize>) {
    let rig = Rig::new().with_inbox();
    let generators = [
        Scripted::new(vec![Reply::text("d1"), Reply::text("d4")]),
        Scripted::new(vec![Reply::text("d2")]),
        Scripted::new(vec![Reply::text("d3")]),
    ];
    let critics = [
        Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]),
        Scripted::new(vec![Reply::answer(false, "m2")]),
        Scripted::new(vec![Reply::answer(true, "")]),
    ];
    deliver_on(&generators[1], 1, arrival(&rig, kind));
    let node = build(
        &rig,
        mixed(&[
            (&generators[0], false),
            (&generators[1], true),
            (&generators[2], false),
        ]),
        rungs(&[&critics[0], &critics[1], &critics[2]]),
        5,
        Setup::default(),
    );

    outcome_of(&node).await;

    (
        calls_of(&[&generators[0], &generators[1], &generators[2]]),
        calls_of(&[&critics[0], &critics[1], &critics[2]]),
    )
}

#[tokio::test(start_paused = true)]
async fn given_rejections_when_rounds_pass_then_round_n_uses_entry_n_of_each_ladder_and_the_last_entry_is_reused()
 {
    let rig = Rig::new();
    let generators = [
        Scripted::new(vec![Reply::text("d1")]),
        Scripted::new(vec![Reply::text("d2")]),
        Scripted::new(vec![
            Reply::text("d3"),
            Reply::text("d4"),
            Reply::text("d5"),
        ]),
    ];
    let critics = [
        Scripted::new(vec![Reply::answer(false, "m1")]),
        Scripted::new(vec![Reply::answer(false, "m2")]),
        Scripted::new(vec![
            Reply::answer(false, "m3"),
            Reply::answer(false, "m4"),
            Reply::answer(true, ""),
        ]),
    ];
    let node = build(
        &rig,
        rungs(&[&generators[0], &generators[1], &generators[2]]),
        rungs(&[&critics[0], &critics[1], &critics[2]]),
        5,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(
        calls_of(&[&generators[0], &generators[1], &generators[2]]),
        vec![1, 1, 3]
    );
    assert_eq!(
        calls_of(&[&critics[0], &critics[1], &critics[2]]),
        vec![1, 1, 3]
    );
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.iterations, Some(5));
}

#[tokio::test(start_paused = true)]
async fn given_a_ladder_of_one_entry_when_rounds_pass_then_every_round_uses_it() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![
        Reply::text("d1"),
        Reply::text("d2"),
        Reply::text("d3"),
    ]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "m1"),
        Reply::answer(false, "m2"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 5, Setup::default());

    outcome_of(&node).await;

    assert_eq!(generator.calls(), 3);
    assert_eq!(critic.calls(), 3);
}

#[tokio::test(start_paused = true)]
async fn given_a_human_message_stored_at_a_pass_start_when_the_pass_picks_its_entries_then_the_counter_is_back_to_zero()
 {
    let (generators, critics) = item_during_round_two(Kind::Human).await;

    assert_eq!(generators, vec![2, 1, 0]);
    assert_eq!(critics, vec![2, 1, 0]);
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_to_the_agent_stored_at_a_pass_start_when_the_pass_picks_its_entries_then_the_counter_is_back_to_zero()
 {
    let (generators, critics) = item_during_round_two(Kind::MailTo).await;

    assert_eq!(generators, vec![2, 1, 0]);
    assert_eq!(critics, vec![2, 1, 0]);
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_in_copy_stored_at_a_pass_start_when_the_pass_picks_its_entries_then_the_counter_is_back_to_zero()
 {
    let (generators, critics) = item_during_round_two(Kind::MailCc).await;

    assert_eq!(generators, vec![2, 1, 0]);
    assert_eq!(critics, vec![2, 1, 0]);
}

#[tokio::test(start_paused = true)]
async fn given_a_runtime_notice_stored_at_a_pass_start_when_the_pass_picks_its_entries_then_the_counter_is_not_reset()
 {
    let (generators, critics) = item_during_round_two(Kind::Notice).await;

    assert_eq!(generators, vec![1, 1, 1]);
    assert_eq!(critics, vec![1, 1, 1]);
}

#[tokio::test(start_paused = true)]
async fn given_a_validation_with_a_waiting_request_item_when_the_new_pass_starts_then_the_counter_is_back_to_zero()
 {
    let rig = Rig::new().with_inbox();
    let (_, deliver) = human_item(&rig, "item");
    let first = Scripted::new(vec![Reply::text("d1"), Reply::text("d3")]);
    let second = Scripted::new(vec![Reply::text("d2")]);
    deliver_on(&second, 1, deliver);
    let c1 = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]);
    let c2 = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(
        &rig,
        mixed(&[(&first, false), (&second, true)]),
        rungs(&[&c1, &c2]),
        5,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(calls_of(&[&first, &second]), vec![2, 1]);
    assert_eq!(calls_of(&[&c1, &c2]), vec![2, 1]);
    assert_eq!(update.judgement, Some(Judgement::Validated));
    assert_eq!(update.iterations, Some(3));
}

#[tokio::test(start_paused = true)]
async fn given_a_request_item_drained_after_a_tool_run_when_the_round_ends_then_the_critic_still_uses_the_entry_read_at_the_pass_start()
 {
    let (tool, _) = echo("lookup");
    let rig = Rig::new().with_inbox().with_tools(vec![tool]);
    let (_, deliver) = human_item(&rig, "item");
    let g1 = Scripted::new(vec![Reply::text("d1")]);
    let g2 = Scripted::new(vec![
        Reply::call("lookup"),
        Reply::text("d2"),
        Reply::text("d3"),
    ]);
    deliver_on(&g2, 1, deliver);
    let c1 = Scripted::new(vec![Reply::answer(false, "m1")]);
    let c2 = Scripted::new(vec![Reply::answer(false, "m2"), Reply::answer(true, "")]);
    let node = build(
        &rig,
        mixed(&[(&g1, false), (&g2, true)]),
        rungs(&[&c1, &c2]),
        5,
        Setup::default(),
    );

    outcome_of(&node).await;

    assert_eq!(calls_of(&[&g1, &g2]), vec![1, 3]);
    assert_eq!(calls_of(&[&c1, &c2]), vec![1, 2]);
}

#[tokio::test(start_paused = true)]
async fn given_a_request_item_waiting_at_the_start_of_the_last_pass_when_it_is_drained_then_the_pass_is_ordinary_and_the_counter_is_back_to_zero()
 {
    for kind in [Kind::Human, Kind::MailTo, Kind::MailCc] {
        let rig = Rig::new().with_inbox();
        let first = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
        let second = Scripted::new(vec![]);
        deliver_on(&first, 1, arrival(&rig, kind));
        let c1 = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(true, "")]);
        let c2 = Scripted::new(vec![]);
        let node = build(
            &rig,
            mixed(&[(&first, true), (&second, false)]),
            rungs(&[&c1, &c2]),
            1,
            Setup::default(),
        );

        let update = outcome_of(&node).await;

        assert_eq!(calls_of(&[&first, &second]), vec![2, 0]);
        assert_eq!(calls_of(&[&c1, &c2]), vec![2, 0]);
        assert_eq!(update.judgement, Some(Judgement::Validated));
        assert_eq!(update.iterations, Some(2));
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_notice_waiting_at_the_start_of_the_last_pass_when_it_is_drained_then_the_pass_stays_the_last_pass()
 {
    let rig = Rig::new().with_inbox();
    let first = Scripted::new(vec![Reply::text("d1")]);
    let second = Scripted::new(vec![Reply::text("d2")]);
    deliver_on(&first, 1, arrival(&rig, Kind::Notice));
    let c1 = Scripted::new(vec![Reply::answer(false, "m1")]);
    let node = build(
        &rig,
        mixed(&[(&first, true), (&second, false)]),
        one(&c1),
        1,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(calls_of(&[&first, &second]), vec![1, 1]);
    assert_eq!(c1.calls(), 1);
    assert_eq!(update.judgement, Some(Judgement::NotJudged));
    assert_eq!(update.text, "d2");
}

#[tokio::test(start_paused = true)]
async fn given_an_earlier_instance_that_stopped_when_a_new_instance_starts_then_its_counters_start_again_at_zero()
 {
    let rig = Rig::new().with_step_limit(2);
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    let critic = Scripted::new(vec![Reply::answer(false, "m1"), Reply::answer(false, "m2")]);
    let setup = Setup {
        round_ending: RoundLimitEnding::KeepRejectedGeneration,
        ..Setup::default()
    };
    let earlier = build(&rig, one(&generator), one(&critic), 2, setup);
    let first = outcome_of(&earlier).await;
    assert_eq!(first.judgement, Some(Judgement::Rejected));
    assert_eq!(first.iterations, Some(2));

    let (h1, h2) = (
        Scripted::new(vec![Reply::text("d3")]),
        Scripted::new(vec![]),
    );
    let validator = Scripted::new(vec![Reply::answer(true, "")]);
    let restart = build(
        &rig,
        rungs(&[&h1, &h2]),
        one(&validator),
        2,
        Setup::default(),
    );
    let second = outcome_of(&restart).await;

    assert_eq!(calls_of(&[&h1, &h2]), vec![1, 0]);
    assert_eq!(second.iterations, Some(1));
    assert_eq!(second.judgement, Some(Judgement::Validated));
    assert!(!second.step_limit);
}
