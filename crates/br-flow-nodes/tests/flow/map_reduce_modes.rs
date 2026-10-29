use std::sync::atomic::Ordering;

use br_flow_nodes::{FaultMode, MapReduceError};
use br_graph_contract::{Event, EventEndError};

use crate::common::{
    BAD_VALUE, CreateMode, Driver, Mode, Probe, Report, capturing_fan_out, counting_fan_out,
    creation_at, fan_out, mode_at,
};

const MODES: [FaultMode; 2] = [FaultMode::NonStrict, FaultMode::Strict];

#[tokio::test(flavor = "multi_thread")]
async fn given_the_mode_strict_when_no_element_has_a_fault_then_reduce_gets_a_state_for_every_element_in_map_order()
 {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let (node, captured) = capturing_fan_out(&probe, 2, FaultMode::Strict);
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    driver.update().await;
    let outs: Vec<_> = captured
        .lock()
        .unwrap()
        .iter()
        .map(|outcome| outcome.state().and_then(|state| state.fields().out))
        .collect();
    assert_eq!(outs, vec![Some(2), Some(4), Some(6)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_the_mode_strict_when_an_element_fails_then_the_node_fails_and_reduce_never_runs() {
    let (probe, _watch) = Probe::with_mode(mode_at(2, Mode::Fail));
    let (node, reduces) = {
        let (node, _maps, reduces) = counting_fan_out(&probe, 1);
        (node.fault_mode(FaultMode::Strict), reduces)
    };
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InternalFailed { position: 1, .. }
    ));
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_any_mode_when_an_initial_value_is_invalid_then_the_node_fails_before_any_instance_is_created()
 {
    for mode in MODES {
        let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
        let mut driver = Driver::start(&fan_out(&probe, 2).fault_mode(mode)).await;

        driver
            .give(Report::with_items(&[1, BAD_VALUE]).state())
            .unwrap();

        assert!(matches!(
            driver.failure().await,
            MapReduceError::InvalidInitialValue { position: 1, .. }
        ));
        assert_eq!(watch.created(), 0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_any_mode_when_an_instance_cannot_be_created_then_the_node_fails_with_its_position() {
    for mode in MODES {
        let (probe, _watch) = Probe::new(creation_at(1, CreateMode::FailCreate), |_| Mode::Answer);
        let mut driver = Driver::start(&fan_out(&probe, 1).fault_mode(mode)).await;

        driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

        assert!(matches!(
            driver.failure().await,
            MapReduceError::InstanceNotCreated { position: 1, .. }
        ));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_any_mode_when_an_instance_carries_another_id_then_the_node_fails_with_its_position()
{
    for mode in MODES {
        let (probe, _watch) = Probe::new(creation_at(0, CreateMode::WrongId), |_| Mode::Answer);
        let mut driver = Driver::start(&fan_out(&probe, 1).fault_mode(mode)).await;

        driver.give(Report::with_items(&[1, 2]).state()).unwrap();

        assert!(matches!(
            driver.failure().await,
            MapReduceError::InstanceIdMismatch { position: 0, .. }
        ));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_any_mode_when_an_instance_closed_its_input_end_then_the_node_fails_with_its_position()
 {
    for mode in MODES {
        let (probe, _watch) = Probe::new(creation_at(2, CreateMode::CloseInput), |_| Mode::Answer);
        let mut driver = Driver::start(&fan_out(&probe, 1).fault_mode(mode)).await;

        driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

        assert!(matches!(
            driver.failure().await,
            MapReduceError::InputRefused { position: 2 }
        ));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_failed_element_when_a_later_instance_cannot_be_created_in_non_strict_then_the_node_still_fails_with_the_creation_fault()
 {
    let (probe, _watch) = Probe::new(
        creation_at(2, CreateMode::FailCreate),
        mode_at(1, Mode::Fail),
    );
    let (node, _maps, reduces) = counting_fan_out(&probe, 1);
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert!(matches!(
        driver.failure().await,
        MapReduceError::InstanceNotCreated { position: 2, .. }
    ));
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_any_mode_when_the_host_cancels_while_an_element_fails_then_cancelled_is_sent_and_reduce_never_runs()
 {
    for mode in MODES {
        let (probe, watch) = Probe::with_mode(|_| Mode::TriggerThenFail);
        let (node, _maps, reduces) = counting_fan_out(&probe, 1);
        let node = node.fault_mode(mode);
        let mut driver = Driver::start(&node).await;
        watch.outer_trigger(driver.cancel.handle().clone());
        driver.give(Report::with_items(&[1]).state()).unwrap();

        let event = driver.event().await;

        assert!(matches!(event, Event::Cancelled), "{event:?}");
        assert_eq!(reduces.load(Ordering::SeqCst), 0);
        assert_eq!(driver.ended().await, EventEndError::Finished);
    }
}
