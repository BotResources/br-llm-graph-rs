use br_graph_contract::Event;

use crate::common::{CreateMode, Driver, Mode, Probe, ProbeCause, Report, fan_out, mode_at};

#[tokio::test(flavor = "multi_thread")]
async fn given_an_internal_instance_that_fails_with_boom_when_the_failed_event_is_displayed_then_the_text_shows_boom_and_the_source_is_still_the_cause()
 {
    let (probe, _watch) = Probe::with_mode(mode_at(1, Mode::Fail));
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;
    driver.give(Report::with_items(&[1]).state()).unwrap();

    let cause = match driver.event().await {
        Event::Failed(cause) => cause,
        other => panic!("expected Failed, got {other:?}"),
    };

    assert!(cause.to_string().contains("boom"));
    assert!(
        cause
            .source()
            .is_some_and(|source| source.is::<ProbeCause>())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn given_an_instance_that_cannot_be_created_when_the_error_is_displayed_then_the_text_shows_the_cause()
 {
    let (probe, _watch) = Probe::new(
        |position| {
            if position == 0 {
                CreateMode::FailCreate
            } else {
                CreateMode::Normal
            }
        },
        |_| Mode::Answer,
    );
    let mut driver = Driver::start(&fan_out(&probe, 1)).await;
    driver.give(Report::with_items(&[1]).state()).unwrap();

    let error = driver.failure().await;

    assert_eq!(
        error.to_string(),
        "the internal instance 0 could not be created: boom"
    );
}
