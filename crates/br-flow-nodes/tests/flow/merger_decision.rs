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
async fn given_a_ready_state_when_an_input_is_read_then_a_result_with_the_empty_update_is_sent_and_the_input_end_is_closed()
 {
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
async fn given_a_state_that_is_not_ready_when_an_input_is_read_then_the_instance_stays_silent() {
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
async fn given_inputs_with_different_states_when_each_is_read_then_each_is_judged_on_its_own_state_with_no_memory()
 {
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
async fn given_a_result_already_sent_when_a_later_input_is_given_then_nothing_more_is_sent_and_the_input_is_refused()
 {
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
async fn given_a_spent_input_with_the_silent_policy_when_it_is_read_then_spent_is_called_first_and_ready_is_not_reached()
 {
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
async fn given_no_late_input_policy_from_the_runner_when_a_spent_input_is_read_then_the_policy_is_silent()
 {
    assert_eq!(LateInput::default(), LateInput::Silent);
    let mut driver = Driver::start(&plain()).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();

    driver.silent().await;
}

#[tokio::test(start_paused = true)]
async fn given_a_silent_late_input_when_a_later_input_is_ready_then_the_instance_still_reads_and_fires()
 {
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
async fn given_the_fail_policy_when_a_spent_input_is_read_then_the_input_end_is_closed_and_a_refusal_with_a_reason_is_sent()
 {
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
async fn given_the_fail_policy_when_an_input_that_is_not_spent_is_read_then_ready_decides() {
    let mut driver = Driver::start(&plain().late_input(LateInput::Fail)).await;

    driver
        .give(Report::filled(true, true, false).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_the_fire_again_policy_when_a_spent_input_is_read_then_ready_decides() {
    let (node, ready_calls, _) = counting(LateInput::FireAgain);
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::filled(true, true, true).state())
        .unwrap();

    assert_eq!(driver.update().await, ReportUpdate::Nothing);
    assert_eq!(ready_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn given_the_fire_again_policy_when_a_spent_input_is_not_ready_then_the_instance_stays_silent()
 {
    let mut driver = Driver::start(&plain().late_input(LateInput::FireAgain)).await;

    driver
        .give(Report::filled(true, false, true).state())
        .unwrap();

    driver.silent().await;
}

#[tokio::test(flavor = "current_thread")]
async fn given_inputs_read_after_the_close_when_the_result_is_sent_then_spent_does_not_judge_them_and_one_result_answers_them_all()
 {
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
async fn given_the_fail_policy_and_several_accepted_inputs_when_they_are_spent_then_one_refusal_answers_them_all()
 {
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
async fn given_two_instances_of_one_node_when_each_reads_its_inputs_then_each_decides_alone() {
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
