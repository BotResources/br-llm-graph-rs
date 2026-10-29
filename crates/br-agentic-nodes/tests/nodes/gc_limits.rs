use br_agentic_nodes::StepLimitEnding;
use br_graph_contract::Event;

use crate::common::*;

#[tokio::test(start_paused = true)]
async fn given_the_default_ending_when_a_pass_stops_at_the_step_limit_then_the_critic_is_not_called_and_no_verdict_is_given()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(1);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::call("search")]);
    let critic = Scripted::new(vec![]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(update.verdict, None);
    assert_eq!(critic.calls(), 0);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(agent_mails(&conversation).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_a_critic_call_when_the_critic_validates_then_the_verdict_and_the_flag_are_given()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(1);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::call("search")]);
    let critic = Scripted::new(vec![Reply::answer(true, "fine as it is")]);
    let node = rig
        .generator_critic(one(&generator), one(&critic), 3)
        .with_step_limit_ending(StepLimitEnding::WithCriticCall);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(update.verdict, Some(true));
    assert_eq!(critic.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_the_ending_with_a_critic_call_when_the_critic_does_not_validate_then_the_instance_fails_with_the_message_and_stores_nothing()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(1);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::call("search")]);
    let critic = Scripted::new(vec![Reply::answer(false, "still unfinished")]);
    let node = rig
        .generator_critic(one(&generator), one(&critic), 3)
        .with_step_limit_ending(StepLimitEnding::WithCriticCall);

    let cause = Driver::failure(&node).await;

    assert!(
        cause.contains("did not validate at the step limit"),
        "{cause}"
    );
    assert!(cause.contains("still unfinished"), "{cause}");
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(agent_mails(&conversation).is_empty());
    assert!(rig.outcome.outcomes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_final_response_at_the_last_step_when_the_pass_ends_then_the_critic_is_called_as_in_every_round()
 {
    let rig = Rig::new().with_step_limit(1);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("done in one")]);
    let critic = Scripted::new(vec![Reply::answer(true, "ok")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert!(!update.step_limit);
    assert_eq!(update.verdict, Some(true));
    assert_eq!(critic.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_does_not_validate_when_no_step_remains_then_the_message_is_stored_and_the_loop_stops()
 {
    let rig = Rig::new().with_step_limit(1);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("done in one"), Reply::text("never")]);
    let critic = Scripted::new(vec![Reply::answer(false, "needs more")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.verdict, Some(false));
    assert!(!update.step_limit);
    assert_eq!(generator.calls(), 1);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(agent_mails(&conversation).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_several_passes_when_the_steps_are_counted_then_the_step_limit_spans_all_of_them() {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]).with_step_limit(3);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![
        Reply::text("first"),
        Reply::call("search"),
        Reply::call("search"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(false, "go on")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 5);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    assert_eq!(update.verdict, None);
    assert_eq!(generator.calls(), 3);
    assert_eq!(critic.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_two_runs_when_the_second_starts_then_the_step_and_round_counters_start_again() {
    let rig = Rig::new().with_step_limit(2);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![
        Reply::text("a"),
        Reply::text("b"),
        Reply::text("c"),
        Reply::text("d"),
    ]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "x"),
        Reply::answer(false, "y"),
        Reply::answer(false, "z"),
        Reply::answer(false, "w"),
    ]);
    let node = rig.generator_critic(one(&generator), one(&critic), 2);

    let (_, first) = Driver::run(&node).await;
    let (_, second) = Driver::run(&node).await;

    assert!(matches!(first, Event::Result(_)));
    assert!(matches!(second, Event::Result(_)));
    assert_eq!(generator.calls(), 4);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_validated_when_an_item_waits_and_a_step_remains_then_the_generator_makes_a_new_pass()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("first"), Reply::text("second")]);
    rig.note_on_first_call(&generator, agent, "extra request");
    let critic = Scripted::new(vec![
        Reply::answer(true, "ok"),
        Reply::answer(true, "ok again"),
    ]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "second");
    assert_eq!((generator.calls(), critic.calls()), (2, 2));
    assert!(history_text(&generator.requests()[1]).contains("extra request"));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_validated_when_no_step_remains_then_the_item_stays_in_the_inbox() {
    let rig = Rig::new().with_inbox().with_step_limit(1);
    let agent = rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("only")]);
    rig.note_on_first_call(&generator, agent, "too late");
    let critic = Scripted::new(vec![Reply::answer(true, "ok")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    Driver::result(&node).await;

    assert!(!format!("{:?}", conversation(&rig.hub, &rig.thread)).contains("too late"));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_does_not_validate_when_the_next_pass_starts_then_the_items_join_the_message_of_the_critic()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("first"), Reply::text("second")]);
    rig.note_on_first_call(&generator, agent, "added meanwhile");
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, "ok"),
    ]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    let inputs = input_turns(&conversation);
    assert_eq!(inputs.len(), 2);
    assert_eq!(inputs[1].entries().len(), 2);
    assert!(history_text(&generator.requests()[1]).contains("added meanwhile"));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_does_not_validate_at_the_round_limit_when_an_item_waits_then_it_stays_in_the_inbox()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("first")]);
    rig.note_on_first_call(&generator, agent, "left waiting");
    let critic = Scripted::new(vec![Reply::answer(false, "no")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 1);

    let update = Driver::result(&node).await;

    assert_eq!(update.verdict, Some(false));
    assert!(!format!("{:?}", conversation(&rig.hub, &rig.thread)).contains("left waiting"));
}

#[tokio::test(start_paused = true)]
async fn given_items_stored_by_a_drain_when_the_next_pass_chooses_its_entry_then_the_counter_is_back_at_zero_and_entry_one_serves()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.start_gc("write").await;
    let g1 = Scripted::new(vec![Reply::text("g1 first"), Reply::text("g1 second")]);
    let g2 = Scripted::new(vec![]);
    let c1 = Scripted::new(vec![Reply::answer(false, "no"), Reply::answer(true, "yes")]);
    let c2 = Scripted::new(vec![]);
    rig.note_on_first_call(&g1, agent, "reset the rounds");
    let node = rig.generator_critic(
        ladder(vec![list(&[&g1]), list(&[&g2])]),
        ladder(vec![list(&[&c1]), list(&[&c2])]),
        5,
    );

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "g1 second");
    assert_eq!(
        (g1.calls(), g2.calls(), c1.calls(), c2.calls()),
        (2, 0, 2, 0)
    );
}

#[tokio::test(start_paused = true)]
async fn given_items_stored_by_a_drain_when_the_round_limit_is_two_then_the_reset_gives_more_rounds()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("a"), Reply::text("b"), Reply::text("c")]);
    rig.note_on_first_call(&generator, agent, "reset the rounds");
    let critic = Scripted::new(vec![
        Reply::answer(false, "x"),
        Reply::answer(false, "y"),
        Reply::answer(true, "z"),
    ]);
    let node = rig.generator_critic(one(&generator), one(&critic), 2);

    let update = Driver::result(&node).await;

    assert_eq!(update.verdict, Some(true));
    assert_eq!(generator.calls(), 3);
}
