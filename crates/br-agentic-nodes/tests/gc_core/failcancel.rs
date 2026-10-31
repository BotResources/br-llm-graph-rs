use std::sync::Arc;
use std::time::Duration;

use br_agentic_contract::FileSystemMode;
use br_agentic_contract::memory::Call;
use br_graph_contract::{CancelToken, Event, Node};
use br_llm_clients_core::ClientError;

use crate::access::*;
use crate::common::*;
use crate::kit::*;
use crate::view::*;

fn closures_of_the_critic(rig: &Rig) -> Vec<String> {
    rig.record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::CloseTurn { turn, closure, .. } if turn.agent == rig.critic => {
                Some(format!("{:?}", closure.reason()))
            }
            _ => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_error_the_fallback_cannot_solve_when_the_pair_runs_then_the_instance_fails_and_the_work_stays()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::Fail(ClientError::Transport)]);
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());

    let reason = failed(&node).await;

    assert!(!reason.is_empty());
    assert_eq!(
        timeline(&rig),
        vec!["in:start", "generator[Completed]:draft"]
    );
    assert_eq!(rig.outcome.calls(), 0);
    assert_eq!(count(&rig, "workspace_save"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_turn_left_open_by_a_failed_hand_over_when_the_instance_fails_then_the_turn_is_closed_failed()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(false, "fix it")]);
    let access = Arc::new(FaultyAccess::new(
        rig.access.clone(),
        rig.critic,
        CriticStore::Fail,
    ));
    let spec = rig.pair_spec(one(&generator), one(&critic), 3);
    let ports = ports_over(&rig, access, rig.prompts.clone());
    let node = pair_over(&rig, ports, spec);

    failed(&node).await;

    let closures = closures_of_the_critic(&rig);
    assert_eq!(closures.len(), 1);
    assert!(closures[0].starts_with("Failed"), "{closures:?}");
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_turn_open_when_the_instance_is_cancelled_then_the_turn_is_closed_cancelled()
{
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::answer(false, "fix it")]);
    let faulty = Arc::new(FaultyAccess::new(
        rig.access.clone(),
        rig.critic,
        CriticStore::Hang,
    ));
    let reached = faulty.reached.clone();
    let spec = rig.pair_spec(one(&generator), one(&critic), 3);
    let ports = ports_over(&rig, faulty, rig.prompts.clone());
    let node = pair_over(&rig, ports, spec);
    let mut driver = Driver::start(&node).await;
    driver.give();

    while !reached.load(std::sync::atomic::Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    let closures = closures_of_the_critic(&rig);
    assert_eq!(closures.len(), 1);
    assert!(closures[0].starts_with("Cancelled"), "{closures:?}");
    assert_eq!(rig.outcome.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_critic_call_running_when_the_instance_is_cancelled_then_nothing_of_it_is_stored() {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let generator = Scripted::new(vec![Reply::text("draft")]);
    let critic = Scripted::new(vec![Reply::Hang]);
    let token = CancelToken::new();
    let handle = token.handle().clone();
    critic.on_call(move |_| handle.trigger());
    let node = build(&rig, one(&generator), one(&critic), 3, Setup::default());
    let instance = node.create(token.view(), instance_id()).await.unwrap();
    let (input, events) = instance.into_parts();
    let mut driver = Driver {
        input,
        events,
        cancel: token,
    };
    driver.give();

    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    assert_eq!(critic.calls(), 1);
    assert_eq!(
        timeline(&rig),
        vec!["in:start", "generator[Completed]:draft"]
    );
    assert_eq!(rig.outcome.calls(), 0);
    assert_eq!(count(&rig, "workspace_save"), 0);
}
