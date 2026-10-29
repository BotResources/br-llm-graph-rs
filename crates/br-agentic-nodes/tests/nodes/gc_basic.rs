use br_agentic_contract::{AGENTIC, AgenticNode, Restart};
use br_graph_contract::Node;
use br_llm_clients_core::OutputMode;
use br_llm_messages::ClosureReason;

use crate::common::*;

#[test]
fn given_a_generator_critic_when_asked_then_it_is_an_agentic_node_that_resets_its_counters() {
    let rig = Rig::new();
    let model = Scripted::new(vec![]);
    let node = rig.generator_critic(one(&model), one(&model), 3);

    assert_eq!((node.kind(), node.subkind()), (AGENTIC, "generator_critic"));
    assert_eq!(node.restart(), Restart::Resets);
    assert!(!node.has_inbox());
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_validates_when_the_pair_runs_then_nothing_of_the_critic_is_stored_and_the_verdict_is_given()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "good")]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "draft");
    assert_eq!(update.verdict, Some(true));
    assert!(!update.step_limit);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(input_turns(&conversation).len(), 1);
    assert_eq!(ai_turns(&conversation).len(), 1);
    assert!(agent_mails(&conversation).is_empty());
    assert!(!format!("{conversation:?}").contains("good"));
    assert_eq!(
        (rig.prompts.asked("generator"), rig.prompts.asked("critic")),
        (1, 1)
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_call_when_it_is_made_then_it_sees_the_request_at_the_end_asks_a_structure_and_uses_no_tool_nor_response()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_tools(vec![search]);
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "good")]);
    let hub = rig.hub.clone();
    let opened = std::sync::Mutex::new(Vec::new());
    let opened = std::sync::Arc::new(opened);
    let seen = opened.clone();
    critic.on_call(move |request| {
        seen.lock()
            .unwrap()
            .push(hub.is_open(request.reply_entry()))
    });
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    Driver::result(&node).await;

    let request = &critic.requests()[0];
    assert!(request.tools().is_empty());
    assert!(matches!(request.output(), OutputMode::Structured(_)));
    assert_eq!(*opened.lock().unwrap(), vec![false]);
    let text = history_text(request);
    assert!(text.contains("write"));
    assert!(text.contains("draft"));
    assert!(text.contains("what do you think of this?"));
    assert!(text.rfind("what do you think of this?") > text.rfind("draft"));
    assert_eq!(
        request.system().map(|s| s.as_str()),
        Some("system prompt of the critic")
    );
    let stored = format!("{:?}", conversation(&rig.hub, &rig.thread));
    assert!(!stored.contains("what do you think of this?"));
    assert_eq!(generator.requests()[0].tools().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_does_not_validate_when_the_pair_runs_then_its_message_is_stored_as_a_mail_and_the_generator_sees_it()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft one"), Reply::text("draft two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "shorter please"),
        Reply::answer(true, "fine"),
    ]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "draft two");
    assert_eq!(update.verdict, Some(true));
    let conversation = conversation(&rig.hub, &rig.thread);
    let mails = agent_mails(&conversation);
    assert_eq!(mails.len(), 1);
    assert_eq!(mails[0].0, "critic");
    assert!(mails[0].1.contains("shorter please"));
    assert_eq!(input_turns(&conversation).len(), 2);
    assert_eq!(ai_turns(&conversation).len(), 2);
    assert!(
        ai_turns(&conversation)
            .iter()
            .all(|turn| turn.author().as_str() == "generator")
    );
    assert!(history_text(&generator.requests()[1]).contains("shorter please"));
    let second_critic = history_text(&critic.requests()[1]);
    assert!(second_critic.contains("shorter please"));
    assert!(second_critic.contains("write"));
    assert!(second_critic.contains("draft two"));
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_never_validates_when_the_round_limit_is_reached_then_the_last_message_is_stored_and_the_verdict_is_false()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "no 1"),
        Reply::answer(false, "no 2"),
    ]);
    let node = rig.generator_critic(one(&generator), one(&critic), 2);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "two");
    assert_eq!(update.verdict, Some(false));
    assert!(!update.step_limit);
    assert_eq!((generator.calls(), critic.calls()), (2, 2));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(agent_mails(&conversation).len(), 2);
    assert!(matches!(
        closure(ai_turns(&conversation)[1]),
        Some(ClosureReason::Completed { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn given_ladders_with_two_entries_when_three_rounds_run_then_entry_n_serves_round_n_and_the_last_entry_is_reused()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let g1 = Scripted::new(vec![Reply::text("g1")]);
    let g2 = Scripted::new(vec![Reply::text("g2a"), Reply::text("g2b")]);
    let c1 = Scripted::new(vec![Reply::answer(false, "c1")]);
    let c2 = Scripted::new(vec![
        Reply::answer(false, "c2a"),
        Reply::answer(true, "c2b"),
    ]);
    let node = rig.generator_critic(
        ladder(vec![list(&[&g1]), list(&[&g2])]),
        ladder(vec![list(&[&c1]), list(&[&c2])]),
        5,
    );

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "g2b");
    assert_eq!(
        (g1.calls(), g2.calls(), c1.calls(), c2.calls()),
        (1, 2, 1, 2)
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_ladder_entry_with_a_fallback_list_when_the_generator_pass_starts_then_the_fallback_applies_to_its_first_call()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let broken = Scripted::new(vec![Reply::Fail(
        br_llm_clients_core::ClientError::Transport,
    )]);
    let spare = Scripted::new(vec![Reply::text("from the spare")]);
    let critic = Scripted::new(vec![Reply::answer(true, "ok")]);
    let node = rig.generator_critic(ladder(vec![list(&[&broken, &spare])]), one(&critic), 3);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "from the spare");
}

#[tokio::test(start_paused = true)]
async fn given_a_generator_critic_when_the_author_is_missing_for_the_critic_then_the_pair_fails_naming_the_critic()
 {
    let rig = Rig::without_authors();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "ok")]);
    let node = rig
        .generator_critic(one(&generator), one(&critic), 3)
        .with_generator_author(author("Gina"));

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("critic"), "{cause}");
    assert_eq!(critic.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_authors_in_the_build_when_the_pair_runs_then_each_agent_signs_with_its_own_name() {
    let rig = Rig::without_authors();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "again"),
        Reply::answer(true, "ok"),
    ]);
    let node = rig
        .generator_critic(one(&generator), one(&critic), 3)
        .with_authors(author("Gina"), author("Carl"));

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(
        ai_turns(&conversation)
            .iter()
            .all(|turn| turn.author().as_str() == "Gina")
    );
    assert_eq!(agent_mails(&conversation)[0].0, "Carl");
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_that_fails_for_good_when_the_pair_runs_then_it_fails_and_the_generator_turn_stays_completed()
 {
    let rig = Rig::new();
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::Fail(
        br_llm_clients_core::ClientError::Transport,
    )]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);

    let cause = Driver::failure(&node).await;

    assert!(cause.contains("connection failed"), "{cause}");
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(matches!(
        closure(ai_turns(&conversation)[0]),
        Some(ClosureReason::Completed { .. })
    ));
    assert!(rig.outcome.outcomes().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_during_the_critic_call_when_it_is_triggered_then_nothing_of_the_call_is_stored()
 {
    let rig = Rig::new().with_timeouts(
        std::time::Duration::from_secs(600),
        std::time::Duration::from_secs(600),
    );
    rig.start_gc("write").await;
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::Hang]);
    let node = rig.generator_critic(one(&generator), one(&critic), 3);
    let mut driver = Driver::start(&node).await;
    driver.give();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    assert_eq!(critic.calls(), 1);

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, br_graph_contract::Event::Cancelled));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(agent_mails(&conversation).is_empty());
    assert_eq!(input_turns(&conversation).len(), 1);
    assert!(rig.outcome.outcomes().is_empty());
}
