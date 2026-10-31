use std::time::Duration;

use br_graph_contract::Event;

use crate::pair::{Pair, calls_for, closures_of, ports_over, turns_of};
use crate::ports::{Checker, Verdict, refuse};
use crate::prelude::*;
use crate::wrap::{OnComplete, Tripwire};

async fn until(condition: impl Fn() -> bool) {
    while !condition() {
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_while_the_turn_of_the_programmatic_critic_is_open_when_the_instance_fails_then_the_turn_closes_failed()
 {
    let rig = Rig::new();
    let wire = Tripwire::new(&rig.access, rig.programmatic, OnComplete::Refuse);
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse("fix it"), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .ports(ports_over(&rig, wire.clone()))
        .build();

    let (_driver, event) = Driver::run(&node).await;

    assert!(matches!(event, Event::Failed(_)));
    assert!(wire.hit());
    let closures = closures_of(&rig, rig.programmatic);
    assert_eq!(closures.len(), 1);
    assert!(matches!(closures[0], ClosureReason::Failed { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_failure_while_the_turn_of_the_critic_is_open_when_the_instance_fails_then_the_turn_closes_failed()
 {
    let rig = Rig::new();
    let wire = Tripwire::new(&rig.access, rig.critic, OnComplete::Refuse);
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .ports(ports_over(&rig, wire.clone()))
        .build();

    let (_driver, event) = Driver::run(&node).await;

    assert!(matches!(event, Event::Failed(_)));
    assert!(wire.hit());
    let closures = closures_of(&rig, rig.critic);
    assert_eq!(closures.len(), 1);
    assert!(matches!(closures[0], ClosureReason::Failed { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_while_the_turn_of_the_programmatic_critic_is_open_when_the_instance_stops_then_the_turn_closes_cancelled()
 {
    let rig = Rig::new();
    let wire = Tripwire::new(&rig.access, rig.programmatic, OnComplete::Hang);
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![refuse("fix it"), Verdict::Accept]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .ports(ports_over(&rig, wire.clone()))
        .build();
    let mut driver = Driver::start(&node).await;
    driver.give();
    until(|| wire.hit()).await;

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    let closures = closures_of(&rig, rig.programmatic);
    assert_eq!(closures.len(), 1);
    assert!(matches!(closures[0], ClosureReason::Cancelled { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_while_the_turn_of_the_critic_is_open_when_the_instance_stops_then_the_turn_closes_cancelled()
 {
    let rig = Rig::new();
    let wire = Tripwire::new(&rig.access, rig.critic, OnComplete::Hang);
    let generator = Scripted::new(vec![Reply::text("one"), Reply::text("two")]);
    let critic = Scripted::new(vec![
        Reply::answer(false, "improve"),
        Reply::answer(true, ""),
    ]);
    let node = Pair::new(&rig, &generator, &critic)
        .ports(ports_over(&rig, wire.clone()))
        .build();
    let mut driver = Driver::start(&node).await;
    driver.give();
    until(|| wire.hit()).await;

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    let closures = closures_of(&rig, rig.critic);
    assert_eq!(closures.len(), 1);
    assert!(matches!(closures[0], ClosureReason::Cancelled { .. }));
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_while_the_critic_call_runs_when_the_instance_stops_then_the_call_is_stopped_and_nothing_of_it_is_stored()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one")]);
    let critic = Scripted::new(vec![Reply::Hang]);
    let node = Pair::new(&rig, &generator, &critic).build();
    let mut driver = Driver::start(&node).await;
    driver.give();
    until(|| critic.calls() == 1).await;

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    assert!(turns_of(&rig, rig.critic).is_empty());
    assert!(calls_for(&rig, rig.critic).is_empty());
    assert_eq!(rig.outcome.calls(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_cancel_while_the_programmatic_critic_port_runs_when_the_instance_stops_then_the_call_is_stopped_and_nothing_of_it_is_stored()
 {
    let rig = Rig::new();
    let generator = Scripted::new(vec![Reply::text("one")]);
    let critic = Scripted::new(vec![Reply::answer(true, "")]);
    let checker = Checker::new(vec![Verdict::Hang]);
    let node = Pair::new(&rig, &generator, &critic)
        .programmatic(&checker, 3)
        .build();
    let mut driver = Driver::start(&node).await;
    driver.give();
    until(|| checker.checked().len() == 1).await;

    driver.cancel.handle().trigger();
    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled));
    assert!(turns_of(&rig, rig.programmatic).is_empty());
    assert_eq!(critic.calls(), 0);
    assert_eq!(rig.outcome.calls(), 0);
}
