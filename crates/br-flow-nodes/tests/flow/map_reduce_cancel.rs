use std::sync::atomic::Ordering;

use br_graph_contract::{Event, EventEndError, InputClosed};

use crate::common::{Driver, Mode, Probe, Report, counting_fan_out, mode_at};

#[tokio::test(flavor = "multi_thread")]
async fn given_no_input_yet_when_the_host_cancels_then_the_input_end_is_closed_and_cancelled_is_sent()
 {
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
async fn given_live_internal_instances_and_elements_left_when_the_host_cancels_then_the_child_token_is_triggered_and_no_other_instance_starts()
 {
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
async fn given_a_live_instance_slow_to_stop_when_the_host_cancels_then_cancelled_waits_for_it_to_stop()
 {
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
async fn given_three_live_instances_slow_to_stop_when_the_host_cancels_then_cancelled_waits_for_the_last_to_stop()
 {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::SlowCancel);
    let (node, _maps, _reduces) = counting_fan_out(&probe, 3);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(3).await;

    driver.cancel.handle().trigger();
    watch.gates.release(1);
    driver.silent().await;
    watch.gates.release(2);
    driver.silent().await;
    watch.gates.release(3);

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(watch.created(), 3);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_cancel_triggered_while_a_result_is_applied_with_elements_left_when_the_next_start_is_due_then_no_instance_is_asked_for()
 {
    let (probe, watch) = Probe::with_mode(mode_at(1, Mode::TriggerOnApply));
    let (node, _maps, reduces) = counting_fan_out(&probe, 1);
    let mut driver = Driver::start(&node).await;
    watch.outer_trigger(driver.cancel.handle().clone());
    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    let event = driver.event().await;

    assert!(matches!(event, Event::Cancelled), "{event:?}");
    assert_eq!(watch.create_calls(), 1);
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn given_a_live_instance_that_answers_while_stopping_when_the_host_cancels_then_its_answer_is_discarded_and_cancelled_is_sent()
 {
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
async fn given_the_last_instance_that_cancels_the_host_before_it_answers_when_it_answers_then_cancelled_is_sent_and_reduce_never_runs()
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
async fn given_a_last_result_whose_application_cancels_the_host_when_it_is_applied_then_cancelled_is_sent_and_reduce_never_runs()
 {
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
async fn given_a_silent_internal_instance_when_the_host_cancels_later_then_the_node_stays_silent_until_then_and_sends_cancelled()
 {
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
