use br_flow_nodes::MapReduce;
use br_graph_contract::{Event, EventEndError, State};

use crate::common::{
    CreateMode, Driver, Item, Mode, Probe, Report, fan_out, items_of, nz, summed, tasks_end,
};

#[tokio::test(flavor = "current_thread")]
async fn given_live_internal_instances_when_the_outer_instance_is_dropped_then_every_internal_instance_is_triggered_and_dropped()
 {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::Gate);
    let driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(2).await;

    drop(driver);

    let mut dropped = watch.dropped_positions(2).await;
    dropped.sort_unstable();
    assert_eq!(dropped, vec![0, 1]);
    assert!(watch.views().iter().all(|view| view.is_triggered()));
    assert!(tasks_end().await);
    assert_eq!(watch.created(), 2);
}

#[tokio::test(flavor = "current_thread")]
async fn given_an_instance_being_created_when_the_outer_instance_is_dropped_then_no_task_is_left() {
    let (probe, watch) = Probe::new(|_| CreateMode::Hang, |_| Mode::Answer);
    let driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2]).state()).unwrap();
    watch.created_at_least(1).await;

    drop(driver);

    assert!(tasks_end().await);
}

#[tokio::test(flavor = "current_thread")]
async fn given_an_instance_being_created_when_the_host_cancels_then_cancelled_is_sent() {
    let (probe, watch) = Probe::new(|_| CreateMode::Hang, |_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2]).state()).unwrap();
    watch.created_at_least(1).await;

    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(watch.created(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn given_no_input_yet_when_the_outer_instance_is_dropped_then_no_task_is_left() {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let driver = Driver::start(&fan_out(&probe, 2)).await;

    drop(driver);

    assert!(tasks_end().await);
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_failing_instance_among_live_siblings_when_the_node_fails_then_the_siblings_are_dropped_and_no_task_is_left()
 {
    let (probe, mut watch) = Probe::with_mode(|value| match value {
        1 => Mode::Fail,
        _ => Mode::Gate,
    });
    let mut driver = Driver::start(&fan_out(&probe, 3)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert!(matches!(driver.event().await, Event::Failed(_)));

    let created = watch.created();
    assert!(created >= 2);
    assert_eq!(
        watch.dropped_positions(created).await,
        (0..created).collect::<Vec<_>>()
    );
    assert!(tasks_end().await);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_map_function_that_panics_when_an_input_arrives_then_the_instance_ends_as_vanished()
{
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let node = MapReduce::new(
        "fan",
        |_: &State<Report>| -> Vec<Item> { panic!("map blew up") },
        probe,
        nz(2),
        summed,
    );
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1]).state()).unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_reduce_function_that_panics_when_every_state_is_final_then_the_instance_ends_as_vanished()
 {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let node = MapReduce::new(
        "fan",
        items_of,
        probe,
        nz(2),
        |_: &[State<Item>]| -> <Report as br_graph_contract::Fields>::Update {
            panic!("reduce blew up")
        },
    );
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
}
