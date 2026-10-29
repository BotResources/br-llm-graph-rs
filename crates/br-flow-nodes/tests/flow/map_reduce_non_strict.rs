use br_flow_nodes::{ElementOutcome, FaultMode};
use br_graph_contract::Event;

use crate::common::{
    APPLY_PANIC, CREATION_GATE, CreateMode, Driver, Mode, Probe, Report, ReportUpdate,
    capturing_fan_out, creation_at, fan_out, mode_at, ordered,
};

async fn run_ordered(mode: Mode) -> Vec<String> {
    let (probe, _watch) = Probe::with_mode(mode_at(2, mode));
    let mut driver = Driver::start(&ordered(&probe, 1)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    match driver.update().await {
        ReportUpdate::Reduced { errors, .. } => errors,
        other => panic!("expected Reduced, got {other:?}"),
    }
}

fn around(failure: &str) -> Vec<String> {
    vec![
        "1:Some(2)".to_owned(),
        format!("failed:{failure}"),
        "3:Some(6)".to_owned(),
    ]
}

#[test]
fn given_the_fault_mode_default_when_it_is_read_then_it_is_non_strict() {
    assert_eq!(FaultMode::default(), FaultMode::NonStrict);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_fails_when_the_mode_is_the_default_then_the_node_sends_a_result_with_the_failure_text_of_that_element()
 {
    assert_eq!(
        run_ordered(Mode::Fail).await,
        around("the internal instance 1 failed: boom")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_refuses_when_the_mode_is_non_strict_then_the_failure_text_holds_the_reason()
 {
    assert_eq!(
        run_ordered(Mode::Refuse).await,
        around("the internal instance 1 refused its input: no thanks")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_cancels_alone_when_the_mode_is_non_strict_then_the_failure_text_says_so()
 {
    assert_eq!(
        run_ordered(Mode::CancelAlone).await,
        around("the internal instance 1 was cancelled while its cancel token was not triggered")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_update_the_state_refuses_when_the_mode_is_non_strict_then_the_failure_text_holds_the_reason_and_no_state_is_given()
 {
    assert_eq!(
        run_ordered(Mode::Reject).await,
        around(
            "the update of the internal instance 1 was not accepted: update refused by the fields: the item refuses this update"
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_update_that_gives_an_invalid_state_when_the_mode_is_non_strict_then_the_failure_text_says_the_state_is_not_valid()
 {
    let errors = run_ordered(Mode::Invalidate).await;

    assert_eq!(errors.len(), 3);
    assert!(
        errors[1].starts_with("failed:the update of the internal instance 1 was not accepted: ")
            && errors[1].contains("not valid"),
        "{errors:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_state_whose_apply_panics_when_the_mode_is_non_strict_then_the_failure_text_holds_the_panic_text()
 {
    assert_eq!(
        run_ordered(Mode::PanicOnApply).await,
        around(&format!(
            "the update of the internal instance 1 was not accepted: {APPLY_PANIC}"
        ))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_vanishes_when_the_mode_is_non_strict_then_the_failure_text_says_it_vanished()
 {
    assert_eq!(
        run_ordered(Mode::Vanish).await,
        around("the internal instance 1 vanished without sending a terminal event")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_the_work_of_an_internal_instance_that_panics_when_the_mode_is_non_strict_then_the_failure_text_says_it_vanished()
 {
    assert_eq!(
        run_ordered(Mode::PanicWork).await,
        around("the internal instance 1 vanished without sending a terminal event")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_failed_element_when_reduce_runs_then_it_gets_a_state_for_the_others_and_a_failure_with_text_only_in_map_order()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(2, Mode::Reject));
    let (node, captured) = capturing_fan_out(&probe, 3, FaultMode::NonStrict);
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    driver.update().await;
    let outcomes = captured.lock().unwrap().clone();
    assert_eq!(outcomes.len(), 3);
    assert_eq!(outcomes[0].state().map(|s| s.fields().out), Some(Some(2)));
    assert_eq!(outcomes[0].failure(), None);
    assert_eq!(outcomes[1].state(), None);
    assert!(
        matches!(&outcomes[1], ElementOutcome::Failed(text) if text.contains("refuses")),
        "{:?}",
        outcomes[1]
    );
    assert_eq!(outcomes[2].state().map(|s| s.fields().out), Some(Some(6)));
}

#[tokio::test(start_paused = true)]
async fn given_a_failed_element_among_live_siblings_when_the_node_goes_on_then_the_child_token_is_not_triggered_and_the_siblings_are_not_cancelled()
 {
    let (probe, mut watch) = Probe::with_mode(|value| match value {
        2 => Mode::Fail,
        _ => Mode::Gate,
    });
    let (node, captured) = capturing_fan_out(&probe, 3, FaultMode::NonStrict);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(3).await;
    assert_eq!(watch.dropped_positions(1).await, vec![1]);

    driver.silent().await;

    assert!(watch.views().iter().all(|view| !view.is_triggered()));
    watch.gates.release(3);
    watch.gates.release(1);
    driver.update().await;
    let outcomes = captured.lock().unwrap().clone();
    assert_eq!(outcomes[0].state().map(|s| s.fields().out), Some(Some(2)));
    assert!(outcomes[1].failure().is_some());
    assert_eq!(outcomes[2].state().map(|s| s.fields().out), Some(Some(6)));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_failed_element_and_elements_left_when_it_is_read_then_its_slot_is_free_and_the_next_element_starts_while_a_sibling_is_still_live()
 {
    let (probe, mut watch) = Probe::with_mode(|value| match value {
        2 => Mode::Fail,
        _ => Mode::Gate,
    });
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let mut started = watch.started_values(3).await;
    started.sort_unstable();

    assert_eq!(started, vec![1, 2, 3]);
    assert_eq!(watch.max_live(), 2);
    watch.gates.release(1);
    watch.gates.release(3);
    assert!(matches!(driver.event().await, Event::Result(_)));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_failed_element_when_the_limit_is_one_then_the_next_elements_start_in_list_order() {
    let (probe, watch) = Probe::with_mode(|value| match value {
        1 | 3 => Mode::Fail,
        _ => Mode::Answer,
    });
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver
        .give(Report::with_items(&[1, 2, 3, 4]).state())
        .unwrap();

    assert!(matches!(driver.event().await, Event::Result(_)));
    assert_eq!(watch.values_in_creation_order(), vec![1, 2, 3, 4]);
    assert_eq!(watch.max_live(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_every_element_failed_when_the_node_ends_then_reduce_gets_only_failures_and_the_node_still_sends_a_result()
 {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Fail);
    let (node, captured) = capturing_fan_out(&probe, 2, FaultMode::NonStrict);
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let update = driver.update().await;
    assert_eq!(
        update,
        ReportUpdate::Reduced {
            sum: 0,
            errors: (0..3)
                .map(|position| format!("the internal instance {position} failed: boom"))
                .collect()
        }
    );
    assert!(
        captured
            .lock()
            .unwrap()
            .iter()
            .all(|o| o.failure().is_some())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_creation_pending_when_a_sibling_fails_in_non_strict_then_the_creation_goes_on_and_the_later_elements_start()
 {
    let (probe, mut watch) = Probe::new(
        creation_at(1, CreateMode::GateThenNormal),
        mode_at(1, Mode::Fail),
    );
    let (node, captured) = capturing_fan_out(&probe, 2, FaultMode::NonStrict);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    assert_eq!(watch.dropped_positions(1).await, vec![0]);

    watch.gates.release(CREATION_GATE);

    driver.update().await;
    let outcomes = captured.lock().unwrap().clone();
    assert!(outcomes[0].failure().is_some());
    assert_eq!(outcomes[1].state().map(|s| s.fields().out), Some(Some(4)));
    assert_eq!(outcomes[2].state().map(|s| s.fields().out), Some(Some(6)));
    assert_eq!(watch.created(), 3);
}
