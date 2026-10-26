use std::sync::atomic::Ordering;

use br_graph_contract::InputClosed;

use crate::common::{Driver, Mode, Probe, Report, ReportUpdate, counting_fan_out};

#[tokio::test(start_paused = true)]
async fn the_instance_waits_for_its_first_input_before_it_calls_the_map_function() {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let (node, maps, _reduces) = counting_fan_out(&probe, 2);
    let mut driver = Driver::start(&node).await;

    driver.silent().await;
    assert_eq!(maps.load(Ordering::SeqCst), 0);
    driver.give(Report::with_items(&[1]).state()).unwrap();

    assert_eq!(
        driver.update().await,
        ReportUpdate::Reduced {
            sum: 2,
            errors: Vec::new()
        }
    );
    assert_eq!(watch.created(), 1);
}

#[tokio::test]
async fn it_works_on_the_state_of_the_last_input_it_had_already_accepted() {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let (node, maps, reduces) = counting_fan_out(&probe, 2);
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[1]).state()).unwrap();
    driver.give(Report::with_items(&[2, 3]).state()).unwrap();
    driver.give(Report::with_items(&[7, 8]).state()).unwrap();

    assert_eq!(
        driver.update().await,
        ReportUpdate::Reduced {
            sum: 30,
            errors: Vec::new()
        }
    );
    assert_eq!(maps.load(Ordering::SeqCst), 1);
    assert_eq!(reduces.load(Ordering::SeqCst), 1);
    assert_eq!(watch.values_in_creation_order(), vec![7, 8]);
}

#[tokio::test(flavor = "multi_thread")]
async fn once_it_works_the_input_end_is_closed_and_a_later_input_is_refused() {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::Gate);
    let (node, _maps, _reduces) = counting_fan_out(&probe, 1);
    let mut driver = Driver::start(&node).await;
    driver.give(Report::with_items(&[5]).state()).unwrap();
    watch.started_values(1).await;

    assert_eq!(
        driver.give(Report::with_items(&[6]).state()),
        Err(InputClosed)
    );
    watch.gates.release(5);

    assert_eq!(
        driver.update().await,
        ReportUpdate::Reduced {
            sum: 10,
            errors: Vec::new()
        }
    );
    assert_eq!(watch.values_in_creation_order(), vec![5]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_instance_after_the_result_works_from_the_newer_state() {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let (node, _maps, _reduces) = counting_fan_out(&probe, 2);
    let mut first = Driver::start(&node).await;
    first.give(Report::with_items(&[1]).state()).unwrap();
    first.update().await;

    let mut second = Driver::start(&node).await;
    second.give(Report::with_items(&[1, 2]).state()).unwrap();

    assert_eq!(
        second.update().await,
        ReportUpdate::Reduced {
            sum: 6,
            errors: Vec::new()
        }
    );
}
