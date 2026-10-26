use br_flow_nodes::MapReduceError;

use crate::common::{
    CREATION_GATE, CreateMode, Driver, Mark, Mode, Probe, Report, creation_at, fan_out, mode_at,
};

#[tokio::test(start_paused = true)]
async fn given_a_creation_pending_when_a_sibling_fails_then_the_failure_is_seen_before_the_creation_ends()
 {
    let (probe, watch) = Probe::new(creation_at(1, CreateMode::Hang), mode_at(1, Mode::Fail));
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let error = driver.failure().await;

    assert!(matches!(
        error,
        MapReduceError::InternalFailed { position: 0, .. }
    ));
    assert_eq!(watch.created(), 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_creation_pending_when_a_sibling_result_is_refused_by_the_state_then_the_node_fails_before_the_creation_ends()
 {
    let (probe, _watch) = Probe::new(creation_at(1, CreateMode::Hang), mode_at(1, Mode::Reject));
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    let error = driver.failure().await;

    assert!(matches!(
        error,
        MapReduceError::UpdateRejected { position: 0, .. }
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_sibling_failure_read_during_a_creation_when_the_creation_would_fail_later_then_the_node_reports_the_sibling_and_not_the_creation()
 {
    let (probe, mut watch) = Probe::new(
        creation_at(1, CreateMode::FailWhenReleased),
        mode_at(1, Mode::Fail),
    );
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.dropped_positions(1).await;
    watch.gates.release(CREATION_GATE);

    let error = driver.failure().await;

    assert!(matches!(
        error,
        MapReduceError::InternalFailed { position: 0, .. }
    ));
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_fault_already_known_when_the_next_start_is_due_then_no_further_instance_is_created()
 {
    let (probe, watch) = Probe::with_mode(|value| match value {
        1 => Mode::Answer,
        _ => Mode::Fail,
    });
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let error = driver.failure().await;

    assert!(matches!(
        error,
        MapReduceError::InternalFailed { position: 1, .. }
    ));
    assert_eq!(watch.created(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn given_live_siblings_when_one_fails_then_the_others_are_triggered_then_dropped_before_the_failure_is_readable()
 {
    let (probe, watch) = Probe::with_mode(|value| match value {
        2 => Mode::Fail,
        _ => Mode::Gate,
    });
    let mut driver = Driver::start(&fan_out(&probe, 3)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    let error = driver.failure_marked(&watch.log).await;

    assert!(matches!(
        error,
        MapReduceError::InternalFailed { position: 1, .. }
    ));
    let delivered = watch.log.index_of(Mark::Delivered).expect("a delivery");
    for sibling in [0, 2] {
        let triggered = watch
            .log
            .index_of(Mark::Triggered(sibling))
            .expect("the sibling saw the trigger");
        let dropped = watch
            .log
            .index_of(Mark::Dropped(sibling))
            .expect("the sibling was dropped");
        assert!(
            triggered < dropped && dropped < delivered,
            "sibling {sibling}: triggered {triggered}, dropped {dropped}, delivered {delivered}"
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn given_an_instance_that_closed_its_input_end_after_a_terminal_event_when_its_input_is_given_then_the_node_fails_with_input_refused()
 {
    let (probe, _watch) = Probe::new(creation_at(0, CreateMode::AnswerAndClose), |_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InputRefused { position: 0 }
    ));
}
