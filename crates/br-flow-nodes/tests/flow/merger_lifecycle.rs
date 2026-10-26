use br_flow_nodes::Merger;
use br_graph_contract::{Event, EventEndError, InputClosed};

use crate::common::{Driver, Report, both_filled, merged, tasks_end};

fn plain() -> Merger<Report> {
    Merger::new("join", both_filled, merged)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_triggered_cancel_token_closes_the_input_end_and_sends_cancelled() {
    let mut driver = Driver::start(&plain()).await;

    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(
        driver.give(Report::filled(true, true, false).state()),
        Err(InputClosed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_instance_that_has_read_inputs_still_sends_cancelled_on_cancel() {
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
async fn a_cancel_triggered_before_the_first_read_wins_over_an_accepted_input() {
    let mut driver = Driver::start(&plain()).await;
    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();
    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
}

#[tokio::test(flavor = "current_thread")]
async fn dropping_both_ends_ends_the_task_of_a_silent_instance() {
    let driver = Driver::start(&plain()).await;
    driver
        .give(Report::filled(true, false, false).state())
        .unwrap();

    drop(driver);

    assert!(tasks_end().await);
}

#[tokio::test(flavor = "current_thread")]
async fn dropping_only_the_event_end_ends_the_task_and_refuses_inputs() {
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
async fn dropping_the_instance_before_the_task_ever_ran_ends_it_without_an_event() {
    let driver = Driver::start(&plain()).await;

    drop(driver);

    assert!(tasks_end().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_panic_in_the_ready_function_ends_the_instance_as_vanished() {
    let node = Merger::new("join", |_| -> bool { panic!("ready blew up") }, merged);
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_panic_in_the_spent_function_ends_the_instance_as_vanished() {
    let node = Merger::new("join", both_filled, |_| -> bool { panic!("spent blew up") });
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
}
