use std::sync::atomic::Ordering;

use br_graph_contract::Event;

use crate::common::{
    CREATION_GATE, CreateMode, Driver, Mode, Probe, Report, ReportUpdate, counting_fan_out,
    creation_at, fan_out, strict_fan_out, tasks_end,
};

#[tokio::test(flavor = "current_thread")]
async fn given_a_sibling_result_applied_while_a_creation_is_pending_when_the_creation_ends_then_the_reduce_holds_that_result()
 {
    let (probe, mut watch) =
        Probe::new(creation_at(1, CreateMode::GateThenNormal), |_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.created_at_least(2).await;
    watch.dropped_positions(1).await;

    watch.gates.release(CREATION_GATE);

    assert_eq!(
        driver.update().await,
        ReportUpdate::Reduced {
            sum: 12,
            errors: Vec::new()
        }
    );
}

#[tokio::test(flavor = "current_thread")]
async fn given_live_siblings_and_a_creation_pending_when_the_host_cancels_then_the_siblings_stop_and_cancelled_is_sent()
 {
    let (probe, mut watch) = Probe::new(creation_at(2, CreateMode::Hang), |_| Mode::Gate);
    let (node, _maps, reduces) = counting_fan_out(&probe, 3);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(2).await;
    watch.created_at_least(3).await;

    driver.cancel.handle().trigger();

    assert!(matches!(driver.event().await, Event::Cancelled));
    assert_eq!(watch.created(), 3);
    assert_eq!(reduces.load(Ordering::SeqCst), 0);
    assert_eq!(watch.dropped_positions(2).await, vec![0, 1]);
}

#[tokio::test(flavor = "current_thread")]
async fn given_live_siblings_and_a_creation_pending_when_the_outer_instance_is_dropped_then_every_sibling_is_dropped()
 {
    let (probe, mut watch) = Probe::new(creation_at(2, CreateMode::Hang), |_| Mode::Gate);
    let driver = Driver::start(&fan_out(&probe, 3)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(2).await;
    watch.created_at_least(3).await;

    drop(driver);

    assert_eq!(watch.dropped_positions(2).await, vec![0, 1]);
    assert!(watch.views().iter().all(|view| view.is_triggered()));
    assert!(tasks_end().await);
}

#[tokio::test(flavor = "current_thread")]
async fn given_live_siblings_and_a_creation_pending_when_a_sibling_fails_then_the_other_siblings_are_dropped()
 {
    let (probe, mut watch) = Probe::new(creation_at(2, CreateMode::Hang), |value| {
        if value == 2 { Mode::Fail } else { Mode::Gate }
    });
    let mut driver = Driver::start(&strict_fan_out(&probe, 3)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert!(matches!(driver.event().await, Event::Failed(_)));
    assert_eq!(watch.dropped_positions(2).await, vec![0, 1]);
    assert!(tasks_end().await);
}
