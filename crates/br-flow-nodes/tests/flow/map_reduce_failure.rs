use std::error::Error;

use br_flow_nodes::MapReduceError;
use br_graph_contract::{EventEndError, InputClosed};

use crate::common::{
    BAD_VALUE, CreateMode, Driver, Mode, Probe, ProbeCause, Report, counting_fan_out, fan_out,
    mode_at, nz,
};

#[tokio::test(flavor = "multi_thread")]
async fn given_an_invalid_initial_value_when_the_node_works_then_it_fails_with_its_position_before_any_instance_is_created()
 {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;

    driver
        .give(Report::with_items(&[1, 2, BAD_VALUE, 4]).state())
        .unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InvalidInitialValue { position: 2, .. }
    ));
    assert_eq!(watch.created(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_initial_value_over_the_internal_size_limit_when_the_node_works_then_it_fails_with_its_position()
 {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let node = fan_out(&probe, 2).internal_size_limit(nz(10));
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InvalidInitialValue { position: 0, .. }
    ));
    assert_eq!(watch.created(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_instance_that_cannot_be_created_when_the_node_works_then_it_fails_with_its_position_and_the_opaque_cause()
 {
    let (probe, _watch) = Probe::new(
        |position| {
            if position == 1 {
                CreateMode::FailCreate
            } else {
                CreateMode::Normal
            }
        },
        |_| Mode::Answer,
    );
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let error = driver.failure().await;
    assert!(error.source().is_some_and(|cause| cause.is::<ProbeCause>()));
    assert!(matches!(
        error,
        MapReduceError::InstanceNotCreated { position: 1, .. }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_instance_with_another_id_than_the_one_given_when_it_is_created_then_the_node_fails_with_its_position()
 {
    let (probe, _watch) = Probe::new(
        |position| {
            if position == 0 {
                CreateMode::WrongId
            } else {
                CreateMode::Normal
            }
        },
        |_| Mode::Answer,
    );
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    match driver.failure().await {
        MapReduceError::InstanceIdMismatch {
            position: 0,
            expected,
            returned,
        } => assert_ne!(expected, returned),
        other => panic!("expected InstanceIdMismatch, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_instance_that_refuses_its_one_input_when_the_input_is_given_then_the_node_fails_with_its_position()
 {
    let (probe, _watch) = Probe::new(
        |position| {
            if position == 2 {
                CreateMode::CloseInput
            } else {
                CreateMode::Normal
            }
        },
        |_| Mode::Answer,
    );
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InputRefused { position: 2 }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_fails_when_the_node_reads_it_then_it_fails_with_its_position_and_the_opaque_cause_and_starts_no_later_element()
 {
    let (probe, watch) = Probe::with_mode(mode_at(3, Mode::Fail));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver
        .give(Report::with_items(&[1, 2, 3, 4]).state())
        .unwrap();

    match driver.failure().await {
        MapReduceError::InternalFailed { position: 2, cause } => {
            assert!(cause.is::<ProbeCause>());
        }
        other => panic!("expected InternalFailed, got {other:?}"),
    }
    assert_eq!(watch.created(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_refuses_when_the_node_reads_it_then_it_fails_with_its_position_and_the_reason()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(2, Mode::Refuse));
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    match driver.failure().await {
        MapReduceError::InternalRefused {
            position: 1,
            reason,
        } => {
            assert_eq!(reason, "no thanks");
        }
        other => panic!("expected InternalRefused, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_cancels_alone_when_the_child_token_is_not_triggered_then_the_node_fails_with_its_position()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(1, Mode::CancelAlone));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InternalCancelled { position: 0 }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_vanishes_when_the_node_reads_it_then_it_fails_with_its_position()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(2, Mode::Vanish));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InternalVanished { position: 1 }
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_update_the_state_refuses_when_it_is_applied_then_the_node_fails_with_its_position()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(2, Mode::Reject));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    match driver.failure().await {
        MapReduceError::UpdateRejected {
            position: 1,
            reason,
        } => {
            assert!(reason.contains("refuses"), "{reason}");
        }
        other => panic!("expected UpdateRejected, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_update_that_gives_an_invalid_state_when_it_is_applied_then_the_node_fails_with_its_position()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(1, Mode::Invalidate));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    match driver.failure().await {
        MapReduceError::UpdateRejected {
            position: 0,
            reason,
        } => {
            assert!(reason.contains("not valid"), "{reason}");
        }
        other => panic!("expected UpdateRejected, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_failing_instance_among_live_siblings_when_the_node_fails_then_the_first_failure_is_reported_the_others_are_dropped_and_the_outer_token_stays_free()
 {
    let (probe, mut watch) = Probe::with_mode(|value| match value {
        2 => Mode::Fail,
        _ => Mode::Gate,
    });
    let (node, _maps, reduces) = counting_fan_out(&probe, 3);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let error = driver.failure().await;
    let child_tokens_triggered = watch.views().iter().all(|view| view.is_triggered());

    assert!(matches!(
        error,
        MapReduceError::InternalFailed { position: 1, .. }
    ));
    assert!(child_tokens_triggered);
    let created = watch.created();
    assert!(created >= 2);
    assert_eq!(
        watch.dropped_positions(created).await,
        (0..created).collect::<Vec<_>>()
    );
    assert!(!driver.cancel.handle().is_triggered());
    assert_eq!(reduces.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_eq!(driver.ended().await, EventEndError::Finished);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_failed_node_when_a_later_input_is_given_then_it_is_refused_as_input_closed() {
    let (probe, _watch) = Probe::with_mode(mode_at(1, Mode::Fail));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;
    driver.give(Report::with_items(&[1]).state()).unwrap();
    driver.failure().await;

    assert_eq!(
        driver.give(Report::with_items(&[1]).state()),
        Err(InputClosed)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn given_the_errors_of_a_rejected_update_and_of_a_failed_instance_when_displayed_then_they_show_the_position_and_only_the_opaque_cause_is_a_source()
 {
    let (rejecting, _watch) = Probe::with_mode(mode_at(2, Mode::Reject));
    let (failing, _watch) = Probe::with_mode(mode_at(1, Mode::Fail));
    let mut rejected = Driver::start(&fan_out(&rejecting, 1)).await;
    let mut failed = Driver::start(&fan_out(&failing, 1)).await;
    rejected.give(Report::with_items(&[1, 2]).state()).unwrap();
    failed.give(Report::with_items(&[1]).state()).unwrap();

    let rejected = rejected.failure().await;
    let failed = failed.failure().await;

    assert_eq!(
        rejected.to_string(),
        "the update of the internal instance 1 was not accepted: update refused by the fields: the item refuses this update"
    );
    assert!(rejected.source().is_none());
    assert_eq!(failed.to_string(), "the internal instance 0 failed");
    assert_eq!(
        failed.source().map(ToString::to_string).as_deref(),
        Some("the probe cause")
    );
}
