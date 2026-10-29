use br_llm_messages::ClosureReason;

use crate::common::*;

#[tokio::test(start_paused = true)]
async fn given_an_item_in_the_inbox_when_the_instance_starts_then_it_is_stored_before_the_first_step()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    rig.deliver_human(agent, "waiting note");
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert!(history_text(&model.requests()[0]).contains("waiting note"));
    let conversation = conversation(&rig.hub, &rig.thread);
    let inputs = input_turns(&conversation);
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].entries().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_an_item_that_arrives_during_a_step_when_the_tool_round_ends_then_it_goes_in_the_open_turn_before_the_next_step()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_inbox().with_tools(vec![search]);
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let model = Scripted::new(vec![Reply::call("search"), Reply::text("done")]);
    let hub = rig.hub.clone();
    let delivered = std::sync::atomic::AtomicBool::new(false);
    let note = rig.deliver_human_later(agent, "late note");
    model.on_call(move |_| {
        if !delivered.swap(true, std::sync::atomic::Ordering::SeqCst) {
            hub.deliver_entry(agent, note.clone());
        }
    });
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert!(history_text(&model.requests()[1]).contains("late note"));
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(
        kinds(ai_turns(&conversation)[0]),
        vec![
            "ModelResponse",
            "RuntimeToolResults",
            "HumanMessage",
            "ModelResponse",
            "TurnClosure"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_item_that_arrives_during_a_final_step_when_a_step_remains_then_a_new_turn_starts_with_a_new_step()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let model = Scripted::new(vec![Reply::text("first"), Reply::text("second")]);
    let hub = rig.hub.clone();
    let note = rig.deliver_human_later(agent, "after the answer");
    let delivered = std::sync::atomic::AtomicBool::new(false);
    model.on_call(move |_| {
        if !delivered.swap(true, std::sync::atomic::Ordering::SeqCst) {
            hub.deliver_entry(agent, note.clone());
        }
    });
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "second");
    assert_eq!(model.calls(), 2);
    let conversation = conversation(&rig.hub, &rig.thread);
    let turns = ai_turns(&conversation);
    assert_eq!(turns.len(), 2);
    assert!(matches!(
        closure(turns[0]),
        Some(ClosureReason::Completed { .. })
    ));
    assert!(matches!(
        closure(turns[1]),
        Some(ClosureReason::Completed { .. })
    ));
    assert!(history_text(&model.requests()[1]).contains("after the answer"));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_round_that_leaves_a_provider_continuation_when_an_item_waits_then_nothing_is_stored_until_the_next_boundary()
 {
    let (search, _) = echo("search");
    let rig = Rig::new().with_inbox().with_tools(vec![search]);
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let model = Scripted::new(vec![
        Reply::call_then_continuation("search"),
        Reply::text("second"),
        Reply::text("third"),
    ]);
    rig.note_on_first_call(&model, agent, "queued note");
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "third");
    assert_eq!(model.calls(), 3);
    assert!(!history_text(&model.requests()[1]).contains("queued note"));
    assert!(history_text(&model.requests()[2]).contains("queued note"));
    let conversation = conversation(&rig.hub, &rig.thread);
    let turns = ai_turns(&conversation);
    assert_eq!(
        kinds(turns[0]),
        vec![
            "ModelResponse",
            "RuntimeToolResults",
            "ModelResponse",
            "TurnClosure"
        ]
    );
    assert_eq!(input_turns(&conversation).len(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_an_item_that_arrives_during_the_last_step_when_no_step_remains_then_it_stays_in_the_inbox()
 {
    let rig = Rig::new().with_inbox().with_step_limit(1);
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let model = Scripted::new(vec![Reply::text("only")]);
    let hub = rig.hub.clone();
    let note = rig.deliver_human_later(agent, "too late");
    model.on_call(move |_| hub.deliver_entry(agent, note.clone()));
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(model.calls(), 1);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(input_turns(&conversation).len(), 1);
    assert_eq!(ai_turns(&conversation).len(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_step_limit_stop_when_an_item_waits_then_it_is_not_stored() {
    let (search, _) = echo("search");
    let rig = Rig::new()
        .with_inbox()
        .with_tools(vec![search])
        .with_step_limit(1);
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let model = Scripted::new(vec![Reply::call("search")]);
    let hub = rig.hub.clone();
    let note = rig.deliver_human_later(agent, "kept in the inbox");
    model.on_call(move |_| hub.deliver_entry(agent, note.clone()));
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert!(update.step_limit);
    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(!format!("{conversation:?}").contains("kept in the inbox"));
}

#[tokio::test(start_paused = true)]
async fn given_a_node_without_an_inbox_when_an_item_waits_then_nothing_is_drained() {
    let rig = Rig::new();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    rig.deliver_human(agent, "never read");
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    assert!(!format!("{conversation:?}").contains("never read"));
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_in_the_inbox_when_it_is_stored_then_it_is_marked_read_afterwards() {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    let other = rig.agent("critic").await;
    rig.start_as(agent, "go").await;
    let mail = rig.send_mail(other, agent, "the subject", Vec::new()).await;
    assert!(!rig.hub.is_read(agent, mail));
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert!(rig.hub.is_read(agent, mail));
    assert!(history_text(&model.requests()[0]).contains("the subject"));
}

#[tokio::test(start_paused = true)]
async fn given_an_item_stored_by_the_drain_when_the_run_restarts_then_it_is_not_stored_twice() {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    rig.deliver_human(agent, "once only");
    let model = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;
    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    assert_eq!(format!("{conversation:?}").matches("once only").count(), 1);
}
