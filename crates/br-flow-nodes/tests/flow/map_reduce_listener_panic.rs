use br_flow_nodes::MapReduce;
use br_graph_contract::EventEndError;

use crate::common::{Driver, PoisonedFirst, Report, items_of, nz, summed};

#[tokio::test(start_paused = true, flavor = "current_thread")]
async fn given_a_listener_that_panics_while_a_creation_is_pending_when_it_ends_then_the_node_ends_as_vanished()
 {
    let node = PoisonedFirst::new();
    let outer = MapReduce::new("fan", items_of, node.clone(), nz(2), summed);
    let mut driver = Driver::start(&outer).await;

    driver.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
    assert_eq!(node.create_calls(), 2);
}

#[tokio::test(start_paused = true, flavor = "current_thread")]
async fn given_a_listener_that_panics_with_no_creation_pending_when_it_ends_then_the_node_ends_as_vanished()
 {
    let node = PoisonedFirst::new();
    let outer = MapReduce::new("fan", items_of, node.clone(), nz(1), summed);
    let mut driver = Driver::start(&outer).await;

    driver.give(Report::with_items(&[1]).state()).unwrap();

    assert_eq!(driver.ended().await, EventEndError::Vanished);
    assert_eq!(node.create_calls(), 1);
}
