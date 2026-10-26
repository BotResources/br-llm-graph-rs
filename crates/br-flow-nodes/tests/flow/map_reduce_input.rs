use std::sync::atomic::Ordering;

use br_graph_contract::InputClosed;

use crate::common::{Driver, Mode, Probe, Report, ReportUpdate, counting_fan_out};

#[tokio::test(start_paused = true)]
async fn given_no_input_yet_when_the_instance_waits_then_the_map_function_is_not_called() {
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

#[tokio::test(flavor = "current_thread")]
async fn given_several_accepted_inputs_when_the_node_starts_to_work_then_it_works_on_the_state_of_the_last()
 {
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
async fn given_a_node_at_work_when_a_later_input_is_given_then_the_input_end_is_closed_and_it_is_refused()
 {
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
async fn given_a_result_already_sent_when_a_new_instance_gets_a_newer_state_then_it_works_from_that_state()
 {
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
