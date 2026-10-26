use br_flow_nodes::MapReduceError;
use br_graph_contract::EventEndError;

use crate::common::{
    APPLY_PANIC, CREATE_PANIC, CreateMode, Driver, EXPLODING_VALUE, Mode, Probe, Report,
    VALIDATE_PANIC, creation_at, fan_out, mode_at, tasks_end,
};

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_node_that_panics_when_it_is_called_to_create_then_the_node_fails_with_its_position_and_the_panic_text_and_drops_the_siblings()
 {
    let (probe, mut watch) = Probe::new(creation_at(2, CreateMode::PanicNow), |_| Mode::Gate);
    let mut driver = Driver::start(&fan_out(&probe, 3)).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let error = driver.failure().await;
    assert!(matches!(
        error,
        MapReduceError::InstanceNotCreated { position: 2, .. }
    ));
    assert_eq!(
        error.to_string(),
        format!("the internal instance 2 could not be created: {CREATE_PANIC}")
    );
    assert_eq!(watch.dropped_positions(2).await, vec![0, 1]);
    assert!(tasks_end().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_node_whose_creation_future_panics_when_it_is_polled_then_the_node_fails_with_its_position_and_the_panic_text_and_drops_the_siblings()
 {
    let (probe, mut watch) = Probe::new(creation_at(2, CreateMode::PanicInFuture), |_| Mode::Gate);
    let mut driver = Driver::start(&fan_out(&probe, 3)).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let error = driver.failure().await;
    assert!(matches!(
        error,
        MapReduceError::InstanceNotCreated { position: 2, .. }
    ));
    assert_eq!(
        error.to_string(),
        format!("the internal instance 2 could not be created: {CREATE_PANIC}")
    );
    assert_eq!(watch.dropped_positions(2).await, vec![0, 1]);
    assert!(tasks_end().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_node_that_panics_with_a_payload_that_is_no_text_when_it_creates_then_the_cause_holds_a_neutral_text()
 {
    let (probe, _watch) = Probe::new(creation_at(0, CreateMode::PanicOpaque), |_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1]).state()).unwrap();

    assert_eq!(
        driver.failure().await.to_string(),
        "the internal instance 0 could not be created: panic without a text message"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_node_that_panics_with_a_literal_text_when_it_creates_then_the_cause_holds_that_text()
 {
    let (probe, _watch) = Probe::new(creation_at(0, CreateMode::PanicStatic), |_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1]).state()).unwrap();

    assert_eq!(
        driver.failure().await.to_string(),
        "the internal instance 0 could not be created: literal text"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_node_that_panics_with_a_payload_whose_drop_panics_when_it_creates_then_the_node_fails_as_not_created_with_the_neutral_text()
 {
    let (probe, _watch) = Probe::new(creation_at(0, CreateMode::PanicPoison), |_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1]).state()).unwrap();

    assert_eq!(
        driver.failure().await.to_string(),
        "the internal instance 0 could not be created: panic without a text message"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_state_whose_apply_panics_when_an_update_is_applied_then_the_node_fails_with_its_position_and_the_panic_text_and_drops_the_siblings()
 {
    let (probe, mut watch) = Probe::with_mode(|value| match value {
        2 => Mode::PanicOnApply,
        _ => Mode::Gate,
    });
    let mut driver = Driver::start(&fan_out(&probe, 3)).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    match driver.failure().await {
        MapReduceError::UpdateRejected {
            position: 1,
            reason,
        } => {
            assert_eq!(reason, APPLY_PANIC);
        }
        other => panic!("expected UpdateRejected, got {other:?}"),
    }
    assert_eq!(watch.dropped_positions(3).await, vec![0, 1, 2]);
    assert!(tasks_end().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_initial_internal_state_whose_validate_panics_when_the_node_works_then_it_fails_with_its_position_and_the_panic_text_before_any_instance_is_created()
 {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;

    driver
        .give(Report::with_items(&[1, EXPLODING_VALUE, 3]).state())
        .unwrap();

    match driver.failure().await {
        MapReduceError::InvalidInitialValue {
            position: 1,
            reason,
        } => {
            assert_eq!(reason, VALIDATE_PANIC);
        }
        other => panic!("expected InvalidInitialValue, got {other:?}"),
    }
    assert_eq!(watch.created(), 0);
    assert!(tasks_end().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_the_work_of_an_internal_instance_that_panics_when_the_node_reads_it_then_the_node_fails_as_internal_vanished()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(2, Mode::PanicWork));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InternalVanished { position: 1 }
    ));
    assert_eq!(driver.ended().await, EventEndError::Finished);
}
