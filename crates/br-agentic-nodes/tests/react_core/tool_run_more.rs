use std::time::Duration;

use crate::common::*;
use crate::kit::*;

const SECOND: Duration = Duration::from_secs(1);

fn text_plan(name: &str) -> Plan {
    Plan::Text(format!("{name} done"))
}

#[tokio::test(start_paused = true)]
async fn given_two_calls_to_the_same_tool_when_the_tool_run_starts_then_the_tool_runs_twice_with_two_results()
 {
    let record_rig = Rig::new();
    let (alpha, spy_alpha) = spy("alpha", text_plan("alpha"), &record_rig.record);
    let rig = record_rig.with_tools(vec![alpha]);
    let model = Model::new(vec![Act::calls(&["alpha", "alpha"]), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(spy_alpha.starts(), 2);
    assert_eq!(completed_tools(&rig.record), vec!["alpha", "alpha"]);
}

#[tokio::test(start_paused = true)]
async fn given_a_call_when_the_tool_runs_then_it_holds_a_view_of_the_cancel_token_of_the_instance()
{
    let record_rig = Rig::new();
    let (alpha, spy_alpha) = spy("alpha", text_plan("alpha"), &record_rig.record);
    let rig = record_rig.with_tools(vec![alpha]);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    let (driver, _) = Driver::run(&node).await;

    assert_eq!(
        spy_alpha.triggered_at_start.lock().unwrap().clone(),
        vec![false]
    );
    let view = spy_alpha.views.lock().unwrap()[0].clone();
    assert!(!view.is_triggered());
    driver.cancel.handle().trigger();
    assert!(view.is_triggered());
}

#[tokio::test(start_paused = true)]
async fn given_calls_that_end_close_together_when_the_results_are_completed_then_the_completions_are_handed_over_one_at_a_time()
 {
    let record_rig = Rig::new();
    let (first, _) = spy(
        "first",
        Plan::After(SECOND, "first done".to_owned()),
        &record_rig.record,
    );
    let (second, _) = spy(
        "second",
        Plan::After(SECOND * 3 / 2, "second done".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![first, second]);
    let model = Model::new(vec![Act::calls(&["first", "second"]), Act::text("done")]);
    let quirks = Quirks {
        complete_result_takes: 2 * SECOND,
        ..Quirks::default()
    };
    let access = Wrapped::new(&rig.access, quirks);
    let node = node_over(&rig, access.clone(), plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(access.peak_completions(), 1);
    assert_eq!(completed_tools(&rig.record), vec!["first", "second"]);
}
