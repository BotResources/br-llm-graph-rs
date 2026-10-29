use std::sync::atomic::Ordering;

use br_flow_nodes::{ElementOutcome, MapReduce};
use br_graph_contract::{EventEndError, Fields};

use crate::common::{
    Driver, Item, Mode, Probe, Report, counting_fan_out, items_of, mode_at, nz, summed, tasks_end,
};

#[tokio::test(flavor = "current_thread")]
async fn given_a_failed_element_among_live_siblings_when_the_outer_instance_is_dropped_then_the_siblings_are_dropped_and_no_terminal_event_is_made()
 {
    let (probe, mut watch) = Probe::with_mode(|value| match value {
        2 => Mode::Fail,
        _ => Mode::Gate,
    });
    let (node, _maps, reduces) = counting_fan_out(&probe, 3);
    let driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(3).await;
    assert_eq!(watch.dropped_positions(1).await, vec![1]);

    drop(driver);

    assert_eq!(watch.dropped_positions(2).await, vec![0, 2]);
    let views = watch.views();
    assert!(views[0].is_triggered() && views[2].is_triggered());
    assert!(tasks_end().await);
    assert_eq!(watch.created(), 3);
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_reduce_function_that_panics_on_failed_outcomes_when_an_element_has_failed_then_the_instance_ends_as_vanished()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(2, Mode::Fail));
    let node = MapReduce::new(
        "fan",
        items_of,
        probe,
        nz(2),
        |outcomes: &[ElementOutcome<Item>]| -> <Report as Fields>::Update {
            if outcomes.iter().any(|outcome| outcome.failure().is_some()) {
                panic!("reduce got a failed outcome");
            }
            summed(outcomes)
        },
    );
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
    assert!(tasks_end().await);
}
