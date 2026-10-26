use br_flow_nodes::MapReduceError;

use crate::common::{
    CREATION_GATE, CreateMode, Driver, Mark, Mode, Probe, Report, fan_out, mode_at,
};

fn creation_at(slow: usize, create: CreateMode) -> impl Fn(usize) -> CreateMode + Send + Sync {
    move |position| {
        if position == slow {
            create
        } else {
            CreateMode::Normal
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_failure_of_a_sibling_is_seen_while_the_next_instance_is_being_created() {
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
async fn a_result_of_a_sibling_is_applied_while_the_next_instance_is_being_created() {
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
async fn a_creation_that_fails_after_a_sibling_failed_reports_the_sibling_not_the_creation() {
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
async fn a_fault_that_is_already_known_stops_the_start_of_the_next_instance() {
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
async fn on_the_first_failure_the_other_instances_are_triggered_then_dropped_before_the_failure_is_readable()
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
async fn an_instance_that_closed_its_input_end_after_a_terminal_event_still_refuses_its_input() {
    let (probe, _watch) = Probe::new(creation_at(0, CreateMode::AnswerAndClose), |_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InputRefused { position: 0 }
    ));
}
