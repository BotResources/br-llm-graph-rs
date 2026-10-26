use br_flow_nodes::Merger;
use br_graph_contract::{Event, EventEndError, InputClosed};

use crate::common::{Driver, Report, both_filled, merged, tasks_end};

fn plain() -> Merger<Report> {
    Merger::new("join", both_filled, merged)
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_triggered_cancel_token_when_the_instance_runs_then_the_input_end_is_closed_and_cancelled_is_sent()
 {
    let mut driver = Driver::start(&plain()).await;

    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(
        driver.give(Report::filled(true, true, false).state()),
        Err(InputClosed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_silent_instance_that_has_read_inputs_when_the_host_cancels_then_cancelled_is_sent()
{
    let mut driver = Driver::start(&plain()).await;
    driver
        .give(Report::filled(true, false, false).state())
        .unwrap();
    driver
        .give(Report::filled(false, true, true).state())
        .unwrap();

    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(driver.ended().await, EventEndError::Finished);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_cancel_triggered_before_the_first_read_when_an_input_is_accepted_then_cancelled_wins()
 {
    let mut driver = Driver::start(&plain()).await;
    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();
    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_silent_instance_when_both_ends_are_dropped_then_its_task_ends() {
    let driver = Driver::start(&plain()).await;
    driver
        .give(Report::filled(true, false, false).state())
        .unwrap();

    drop(driver);

    assert!(tasks_end().await);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_silent_instance_when_only_the_event_end_is_dropped_then_its_task_ends_and_inputs_are_refused()
 {
    let driver = Driver::start(&plain()).await;
    let Driver { input, events, .. } = driver;

    drop(events);

    assert!(tasks_end().await);
    assert_eq!(
        input.give(br_graph_contract::Input::new(std::sync::Arc::new(
            Report::filled(true, true, false).state()
        ))),
        Err(InputClosed)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn given_an_instance_that_never_ran_when_it_is_dropped_then_its_task_ends_without_an_event() {
    let driver = Driver::start(&plain()).await;

    drop(driver);

    assert!(tasks_end().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_ready_function_that_panics_when_an_input_is_read_then_the_instance_ends_as_vanished()
 {
    let node = Merger::new("join", |_| -> bool { panic!("ready blew up") }, merged);
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_spent_function_that_panics_when_an_input_is_read_then_the_instance_ends_as_vanished()
 {
    let node = Merger::new("join", both_filled, |_| -> bool { panic!("spent blew up") });
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
}
