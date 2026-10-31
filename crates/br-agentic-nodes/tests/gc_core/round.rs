use std::sync::{Arc, Mutex};

use br_agentic_nodes::Judgement;
use br_llm_messages::{IncomingEntry, Turn};

use crate::common::*;
use crate::items::*;
use crate::kit::*;
use crate::view::*;

fn log_calls(log: &Arc<Mutex<Vec<&'static str>>>, scripted: &Arc<Scripted>, who: &'static str) {
    let log = log.clone();
    scripted.on_call(move |_| log.lock().unwrap().push(who));
}

#[tokio::test(start_paused = true)]
async fn given_two_rounds_when_the_pair_runs_then_the_critic_is_called_once_after_each_pass() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let log = Arc::new(Mutex::new(Vec::new()));
    log_calls(&log, &generator, "generator");
    log_calls(&log, &critic, "critic");
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(
        *log.lock().unwrap(),
        vec!["generator", "critic", "generator", "critic"]
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_pass_when_the_critic_is_called_then_the_turn_of_the_generator_is_already_closed() {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let access = rig.access.clone();
    let probe = seen.clone();
    critic.on_call(move |_| {
        let conversation = access.conversation();
        let closed = ai_turns(&conversation)
            .last()
            .map(|turn| closure(turn).is_some());
        probe.lock().unwrap().push(closed);
    });
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(*seen.lock().unwrap(), vec![Some(true)]);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_call_when_the_projection_is_built_then_the_critic_request_closes_it_and_is_never_stored()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    for request in critic.requests() {
        let last = request.history().turns().last().unwrap();
        let entry = match last {
            Turn::InputTurn(input) => input.entries().last().unwrap().clone(),
            other => panic!("{other:?}"),
        };
        match entry {
            IncomingEntry::HumanMessage(message) => {
                assert!(format!("{:?}", message.content()).contains("what do you think of this?"));
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(
        timeline(&rig)
            .iter()
            .all(|line| !line.contains("what do you think of this?"))
    );
    assert!(
        generator
            .requests()
            .iter()
            .all(|request| !format!("{:?}", request.history())
                .contains("what do you think of this?"))
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_validating_critic_when_the_pair_runs_then_the_critic_call_opens_no_response_and_stores_no_turn()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(count(&rig, "open_response"), 1);
    assert_eq!(count(&rig, "complete_response"), 1);
    assert_eq!(turns_of(&rig, "critic"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_rejecting_critic_when_the_pair_runs_then_its_message_is_completed_without_being_opened()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft"), Reply::text("better")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "fix it"),
        Reply::answer(true, ""),
    ]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    outcome_of(&node).await;

    assert_eq!(count(&rig, "open_response"), 2);
    assert_eq!(count(&rig, "complete_response"), 3);
    assert_eq!(count(&rig, "response_delta"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_pass_of_three_steps_when_the_critic_validates_then_the_critic_is_called_once_and_the_pass_is_one_iteration()
 {
    let (tool, probe) = echo("lookup");
    let rig = Rig::new().with_tools(vec![tool]);
    let generator = Scripted::new(vec![
        Reply::call("lookup"),
        Reply::call("lookup"),
        Reply::text("done"),
    ]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let update = outcome_of(&node).await;

    assert_eq!(generator.calls(), 3);
    assert_eq!(*probe.starts.lock().unwrap(), 2);
    assert_eq!(critic.calls(), 1);
    assert_eq!(update.iterations, Some(1));
    assert_eq!(update.judgement, Some(Judgement::Validated));
}

#[tokio::test(start_paused = true)]
async fn given_an_item_in_the_inbox_when_a_pass_ends_with_a_final_response_then_the_critic_is_called_before_the_queue_is_drained()
 {
    let rig = Rig::new().with_inbox();
    let (_, deliver) = human_item(&rig, "later");
    let generator = Scripted::new(vec![Reply::text("d1"), Reply::text("d2")]);
    deliver_on(&generator, 1, deliver);
    let critic = Scripted::new(vec![Reply::answer(true, ""), Reply::answer(true, "")]);
    let stored_when_called = Arc::new(Mutex::new(Vec::new()));
    let access = rig.access.clone();
    let probe = stored_when_called.clone();
    critic.on_call(move |_| {
        probe
            .lock()
            .unwrap()
            .push(input_turns(&access.conversation()).len());
    });
    let node = build(
        &rig,
        yielding(&generator),
        one(&critic),
        3,
        Setup::default(),
    );

    let update = outcome_of(&node).await;

    assert_eq!(*stored_when_called.lock().unwrap(), vec![1, 2]);
    assert_eq!(update.iterations, Some(2));
    assert_eq!(
        timeline(&rig),
        vec![
            "in:start",
            "generator[Completed]:d1",
            "in:later",
            "generator[Completed]:d2"
        ]
    );
}
