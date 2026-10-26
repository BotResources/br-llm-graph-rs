use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use br_flow_nodes::{LateInput, Merger};
use br_graph_contract::{Event, EventEndError, InputClosed, Node};

use crate::common::{Driver, Report, ReportUpdate, both_filled, merged};

fn plain() -> Merger<Report> {
    Merger::new("join", both_filled, merged)
}

fn counting(late: LateInput) -> (Merger<Report>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    let ready_calls = Arc::new(AtomicUsize::new(0));
    let spent_calls = Arc::new(AtomicUsize::new(0));
    let ready_counter = Arc::clone(&ready_calls);
    let spent_counter = Arc::clone(&spent_calls);
    let node = Merger::new(
        "join",
        move |state| {
            ready_counter.fetch_add(1, Ordering::SeqCst);
            both_filled(state)
        },
        move |state| {
            spent_counter.fetch_add(1, Ordering::SeqCst);
            merged(state)
        },
    )
    .late_input(late);
    (node, ready_calls, spent_calls)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ready_state_gets_a_result_with_the_empty_update_and_closes_the_input_end() {
    let mut driver = Driver::start(&plain()).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
    assert_eq!(
        driver.give(Report::filled(true, true, false).state()),
        Err(InputClosed)
    );
}

#[tokio::test(start_paused = true)]
async fn a_state_that_is_not_ready_leaves_the_instance_silent() {
    let mut driver = Driver::start(&plain()).await;

    driver
        .give(Report::filled(true, false, false).state())
        .unwrap();
    driver
        .give(Report::filled(false, true, false).state())
        .unwrap();

    driver.silent().await;
}

#[tokio::test(start_paused = true)]
async fn each_input_is_judged_on_its_own_state_with_no_memory_between_inputs() {
    let mut driver = Driver::start(&plain()).await;

    driver
        .give(Report::filled(true, false, false).state())
        .unwrap();
    driver
        .give(Report::filled(false, true, false).state())
        .unwrap();
    driver.silent().await;
    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
}

#[tokio::test(flavor = "multi_thread")]
async fn after_its_result_the_instance_sends_nothing_more_and_refuses_inputs() {
    let mut driver = Driver::start(&plain()).await;
    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();
    driver.update().await;

    assert_eq!(driver.ended().await, EventEndError::Finished);
    assert_eq!(
        driver.give(Report::filled(true, true, false).state()),
        Err(InputClosed)
    );
}

#[tokio::test(start_paused = true)]
async fn the_spent_function_is_called_first_and_a_silent_late_input_does_not_reach_ready() {
    let (node, ready_calls, spent_calls) = counting(LateInput::Silent);
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();

    driver.silent().await;
    assert_eq!(spent_calls.load(Ordering::SeqCst), 1);
    assert_eq!(ready_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn the_late_input_policy_is_silent_when_the_runner_gives_none() {
    assert_eq!(LateInput::default(), LateInput::Silent);
    let mut driver = Driver::start(&plain()).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();

    driver.silent().await;
}

#[tokio::test(start_paused = true)]
async fn a_silent_late_input_leaves_the_instance_reading_and_a_later_input_can_fire() {
    let mut driver = Driver::start(&plain().late_input(LateInput::Silent)).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();
    driver.silent().await;
    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fail_policy_closes_the_input_end_and_refuses_a_late_input_with_a_reason() {
    let mut driver = Driver::start(&plain().late_input(LateInput::Fail)).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();

    match driver.event().await {
        Event::Refused(reason) => {
            assert!(reason.as_str().contains("join"), "{reason}");
        }
        other => panic!("expected Refused, got {other:?}"),
    }
    assert_eq!(
        driver.give(Report::filled(true, true, false).state()),
        Err(InputClosed)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fail_policy_leaves_an_input_that_is_not_spent_to_the_ready_function() {
    let mut driver = Driver::start(&plain().late_input(LateInput::Fail)).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_fire_again_policy_goes_on_to_the_ready_function_when_the_input_is_spent() {
    let (node, ready_calls, _) = counting(LateInput::FireAgain);
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
    assert_eq!(ready_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn the_fire_again_policy_stays_silent_when_a_spent_input_is_not_ready() {
    let mut driver = Driver::start(&plain().late_input(LateInput::FireAgain)).await;

    driver
        .give(Report::filled(true, false, true).state())
        .unwrap();

    driver.silent().await;
}

#[tokio::test(flavor = "current_thread")]
async fn inputs_read_after_the_close_are_not_judged_by_spent_and_one_result_answers_them_all() {
    let (node, ready_calls, spent_calls) = counting(LateInput::Fail);
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();
    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();
    driver
        .give(Report::filled(false, false, true).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
    assert_eq!(spent_calls.load(Ordering::SeqCst), 1);
    assert_eq!(ready_calls.load(Ordering::SeqCst), 1);
    assert_eq!(driver.ended().await, EventEndError::Finished);
}

#[tokio::test(flavor = "current_thread")]
async fn the_fail_policy_sends_one_refusal_for_all_the_accepted_inputs() {
    let mut driver = Driver::start(&plain().late_input(LateInput::Fail)).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();
    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert!(matches!(driver.event().await, Event::Refused(_)));
    assert_eq!(driver.ended().await, EventEndError::Finished);
}

#[tokio::test(flavor = "multi_thread")]
async fn two_instances_of_one_node_decide_alone_from_their_own_inputs() {
    let node = Arc::new(plain());
    let mut first = Driver::start(&*node as &dyn Node<Report>).await;
    let mut second = Driver::start(&*node as &dyn Node<Report>).await;

    first
        .give(Report::filled(true, true, false).state())
        .unwrap();
    second
        .give(Report::filled(true, false, false).state())
        .unwrap();

    assert_eq!(first.update().await, ReportUpdate::Nothing);
    second.cancel.handle().trigger();
    assert!(matches!(second.event().await, Event::Cancelled));
}
