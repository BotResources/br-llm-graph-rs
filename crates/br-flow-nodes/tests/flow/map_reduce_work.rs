use std::collections::HashSet;
use std::sync::atomic::Ordering;

use br_flow_nodes::MapReduce;
use br_graph_contract::{Event, State};

use crate::common::{
    Driver, Item, Mode, Probe, Report, ReportUpdate, counting_fan_out, fan_out, nz, ordered, summed,
};

fn reduced(sum: u32, errors: &[&str]) -> ReportUpdate {
    ReportUpdate::Reduced {
        sum,
        errors: errors.iter().map(|text| (*text).to_owned()).collect(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_list_of_elements_when_the_node_works_then_each_runs_through_the_internal_node_and_reduce_joins_the_results()
 {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::Answer);
    let mut driver = Driver::start(&ordered(&probe, 2)).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert_eq!(
        driver.update().await,
        reduced(0, &["1:Some(2)", "2:Some(4)", "3:Some(6)"])
    );
    assert_eq!(watch.values_in_creation_order(), vec![1, 2, 3]);
    assert_eq!(watch.dropped_positions(3).await, vec![0, 1, 2]);
    assert_eq!(watch.extra_inputs(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_instances_that_end_in_reverse_order_when_reduce_runs_then_it_gets_the_states_in_map_order()
 {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::Gate);
    let mut driver = Driver::start(&ordered(&probe, 3)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
    watch.started_values(3).await;

    for value in [3, 2, 1] {
        watch.gates.release(value);
        assert_eq!(watch.dropped_positions(1).await, vec![value as usize - 1]);
    }

    assert_eq!(
        driver.update().await,
        reduced(0, &["1:Some(2)", "2:Some(4)", "3:Some(6)"])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_limit_below_the_number_of_elements_when_the_node_works_then_at_most_the_limit_run_at_once_and_the_next_starts_in_list_order_when_one_ends()
 {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::Gate);
    let mut driver = Driver::start(&ordered(&probe, 2)).await;
    driver
        .give(Report::with_items(&[1, 2, 3, 4, 5]).state())
        .unwrap();

    let mut first_two = watch.started_values(2).await;
    first_two.sort_unstable();
    assert_eq!(first_two, vec![1, 2]);
    assert_eq!(watch.created(), 2);
    watch.gates.release(1);
    assert_eq!(watch.started_values(1).await, vec![3]);
    assert_eq!(watch.created(), 3);
    for value in [2, 3, 4, 5] {
        watch.gates.release(value);
    }

    assert_eq!(
        driver.update().await,
        reduced(
            0,
            &[
                "1:Some(2)",
                "2:Some(4)",
                "3:Some(6)",
                "4:Some(8)",
                "5:Some(10)"
            ]
        )
    );
    assert_eq!(watch.max_live(), 2);
    assert_eq!(watch.values_in_creation_order(), vec![1, 2, 3, 4, 5]);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_limit_above_the_number_of_elements_when_the_node_works_then_all_run_at_once() {
    let (probe, mut watch) = Probe::with_mode(|_| Mode::Gate);
    let mut driver = Driver::start(&fan_out(&probe, 10)).await;
    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    watch.started_values(3).await;
    for value in [1, 2, 3] {
        watch.gates.release(value);
    }

    assert_eq!(driver.update().await, reduced(12, &[]));
    assert_eq!(watch.max_live(), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_a_limit_of_one_when_the_node_works_then_the_elements_run_one_after_the_other() {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;

    driver.give(Report::with_items(&[4, 5, 6]).state()).unwrap();

    assert_eq!(driver.update().await, reduced(30, &[]));
    assert_eq!(watch.max_live(), 1);
    assert_eq!(watch.values_in_creation_order(), vec![4, 5, 6]);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_empty_list_when_the_node_works_then_reduce_gets_an_empty_list_and_no_instance_is_created()
 {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let (node, maps, reduces) = counting_fan_out(&probe, 3);
    let mut driver = Driver::start(&node).await;

    driver.give(Report::with_items(&[]).state()).unwrap();

    assert_eq!(driver.update().await, reduced(0, &[]));
    assert_eq!(watch.created(), 0);
    assert_eq!(maps.load(Ordering::SeqCst), 1);
    assert_eq!(reduces.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_several_elements_when_the_node_works_then_reduce_is_called_once_after_every_internal_state_is_final()
 {
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let (node, _maps, reduces) = counting_fan_out(&probe, 2);
    let mut driver = Driver::start(&node).await;

    driver
        .give(Report::with_items(&[1, 2, 3, 4]).state())
        .unwrap();
    driver.update().await;

    assert_eq!(reduces.load(Ordering::SeqCst), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn given_several_elements_and_runs_when_instances_are_created_then_each_gets_a_fresh_uuid_v7_never_reused()
 {
    let (probe, watch) = Probe::with_mode(|_| Mode::Answer);
    let node = fan_out(&probe, 2);

    for _ in 0..2 {
        let mut driver = Driver::start(&node).await;
        driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();
        driver.update().await;
    }

    let ids = watch.ids();
    assert_eq!(ids.len(), 6);
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), 6);
    assert!(ids.iter().all(|id| id.as_uuid().get_version_num() == 7));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_element_with_an_expected_error_when_it_answers_then_the_error_is_a_result_the_reduce_function_receives()
 {
    let (probe, _watch) = Probe::with_mode(|value| {
        if value == 2 {
            Mode::Record
        } else {
            Mode::Answer
        }
    });
    let mut driver = Driver::start(&fan_out(&probe, 2)).await;

    driver.give(Report::with_items(&[1, 2, 3]).state()).unwrap();

    assert_eq!(driver.update().await, reduced(8, &["could not read"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_size_limit_that_defaults_to_the_contract_one_when_internal_states_are_created_then_it_is_the_limit_they_are_created_with()
 {
    let big = "x".repeat(140_000);
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let node = |internal_size_limit: Option<usize>| {
        let node = MapReduce::new(
            "fan",
            {
                let big = big.clone();
                move |_: &State<Report>| vec![Item::labelled(1, &big)]
            },
            probe.clone(),
            nz(1),
            summed,
        );
        match internal_size_limit {
            Some(limit) => node.internal_size_limit(nz(limit)),
            None => node,
        }
    };

    let mut default = Driver::start(&node(None)).await;
    default.give(Report::blank().state()).unwrap();
    let mut raised = Driver::start(&node(Some(1_000_000))).await;
    raised.give(Report::blank().state()).unwrap();

    assert!(matches!(default.event().await, Event::Failed(_)));
    assert_eq!(raised.update().await, reduced(2, &[]));
}

#[tokio::test(flavor = "multi_thread")]
async fn given_several_runs_of_one_node_at_the_same_time_when_they_work_then_they_do_not_interfere()
{
    let (probe, _watch) = Probe::with_mode(|_| Mode::Answer);
    let node = fan_out(&probe, 2);
    let mut first = Driver::start(&node).await;
    let mut second = Driver::start(&node).await;

    first.give(Report::with_items(&[1, 2]).state()).unwrap();
    second
        .give(Report::with_items(&[10, 20, 30]).state())
        .unwrap();

    assert_eq!(first.update().await, reduced(6, &[]));
    assert_eq!(second.update().await, reduced(120, &[]));
}
