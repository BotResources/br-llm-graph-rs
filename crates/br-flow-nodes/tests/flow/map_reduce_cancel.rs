use std::sync::atomic::Ordering;

use br_graph_contract::{Event, EventEndError, InputClosed};

use crate::common::{Driver, Mode, Probe, Report, counting_fan_out};

#[tokio::test(flavor = "multi_thread")]
async fn cancel_before_the_first_input_closes_the_input_end_and_sends_cancelled() {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let (node, maps, _reduces) = counting_fan_out(&probe, 2);
    let mut driver = Driver::start(&node).await;

    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(
        driver.give(Report::with_items(&[1]).state()),
        Err(InputClosed)
    );
    assert_eq!(maps.load(Ordering::SeqCst), 0);
    assert_eq!(watch.created(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_reaches_the_internal_instances_through_a_child_token_and_starts_no_other() {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::Gate);
    let (node, _maps, reduces) = counting_fan_out(&probe, 2);
    let mut driver = Driver::start(&node).await;
    driver
        .give(Report::with_items(&[1, 2, 3, 4]).state())
        .unwrap();
    watch.started_values(2).await;

    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(watch.created(), 2);
    assert!(watch.views().iter().all(|view| view.is_triggered()));
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn cancel_waits_for_the_live_instances_to_stop_before_it_sends_cancelled() {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::SlowCancel);
    let (node, _maps, _reduces) = counting_fan_out(&probe, 1);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2]).state()).unwrap();
    watch.started_values(1).await;

    driver.cancel.handle().trigger();
    driver.silent().await;
    watch.gates.release(1);

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(watch.created(), 1);
}

#[tokio::test(start_paused = true)]
async fn cancel_discards_what_the_live_instances_send_while_they_stop() {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::IgnoreCancel);
    let (node, _maps, reduces) = counting_fan_out(&probe, 1);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2]).state()).unwrap();
    watch.started_values(1).await;

    driver.cancel.handle().trigger();
    driver.silent().await;
    watch.gates.release(1);

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
    assert_eq!(watch.created(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_triggered_by_the_last_instance_before_it_answers_gives_cancelled_and_never_reduces()
 {
    let (probe, watch) = Probe::with_mode(|_| Mode::TriggerThenAnswer);
    let (node, _maps, reduces) = counting_fan_out(&probe, 1);
    let mut driver = Driver::start(&node).await;
    watch.outer_trigger(driver.cancel.handle().clone());
    driver.give(Report::with_items(&[1]).state()).unwrap();

    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled), "{event:?}");
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
    assert_eq!(driver.ended().await, EventEndError::Finished);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_triggered_while_the_last_result_is_applied_gives_cancelled_and_never_reduces() {
    let (probe, watch) = Probe::with_mode(|_| Mode::TriggerOnApply);
    let (node, _maps, reduces) = counting_fan_out(&probe, 1);
    let mut driver = Driver::start(&node).await;
    watch.outer_trigger(driver.cancel.handle().clone());
    driver.give(Report::with_items(&[1]).state()).unwrap();

    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled), "{event:?}");
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
    assert_eq!(driver.ended().await, EventEndError::Finished);
}

#[tokio::test(start_paused = true)]
async fn a_silent_internal_instance_keeps_the_node_silent_until_the_host_cancels() {
    let (probe, mut watch) = Probe::with_mode(|value| {
        if value == 2 {
            Mode::Silent
        } else {
            Mode::Answer
        }
    });
    let (node, _maps, reduces) = counting_fan_out(&probe, 2);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(3).await;

    driver.silent().await;
    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
}
