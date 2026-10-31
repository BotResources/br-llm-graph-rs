use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::Barrier;

use crate::common::*;
use crate::kit::*;

const SECOND: Duration = Duration::from_secs(1);

fn text_plan(name: &str) -> Plan {
    Plan::Text(format!("{name} done"))
}

#[tokio::test(start_paused = true)]
async fn given_three_calls_when_the_tool_run_starts_then_every_result_is_open_before_any_call_starts()
 {
    let record_rig = Rig::new();
    let (a, spy_a) = spy("a", text_plan("a"), &record_rig.record);
    let (b, spy_b) = spy("b", text_plan("b"), &record_rig.record);
    let (c, spy_c) = spy("c", text_plan("c"), &record_rig.record);
    let rig = record_rig.with_tools(vec![a, b, c]);
    let model = Model::new(vec![Act::calls(&["a", "b", "c"]), Act::text("done")]);
    let quirks = Quirks {
        open_result_yields: true,
        ..Quirks::default()
    };
    let node = node_over(&rig, Wrapped::new(&rig.access, quirks), plains(&[&model]));

    Driver::result(&node).await;

    for seen in [&spy_a, &spy_b, &spy_c] {
        let started = seen.started.lock().unwrap();
        assert_eq!(started.len(), 1);
        assert_eq!(
            started[0],
            Counts {
                opened: 3,
                completed: 0
            }
        );
    }
}

#[tokio::test(start_paused = true)]
async fn given_two_calls_when_the_tool_run_starts_then_the_calls_run_together() {
    let record_rig = Rig::new();
    let gate = Arc::new(Barrier::new(2));
    let (a, _) = spy("a", Plan::Gate(gate.clone()), &record_rig.record);
    let (b, _) = spy("b", Plan::Gate(gate), &record_rig.record);
    let rig = record_rig.with_tools(vec![a, b]);
    let model = Model::new(vec![Act::calls(&["a", "b"]), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let conversation = rig.conversation();
    let results = texts_of_results(ai_turns(&conversation)[0]);
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|(is_error, _)| !is_error), "{results:?}");
}

#[tokio::test(start_paused = true)]
async fn given_calls_of_different_length_when_they_end_then_the_results_are_completed_in_the_order_the_calls_end()
 {
    let record_rig = Rig::new();
    let (slow, spy_slow) = spy(
        "slow",
        Plan::After(3 * SECOND, "slow done".to_owned()),
        &record_rig.record,
    );
    let (fast, spy_fast) = spy(
        "fast",
        Plan::After(SECOND, "fast done".to_owned()),
        &record_rig.record,
    );
    let (mid, spy_mid) = spy(
        "mid",
        Plan::After(2 * SECOND, "mid done".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![slow, fast, mid]);
    let model = Model::new(vec![
        Act::calls(&["slow", "fast", "mid"]),
        Act::text("done"),
    ]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(completed_tools(&rig.record), vec!["fast", "mid", "slow"]);
    assert_eq!(spy_fast.ended.lock().unwrap()[0].completed, 0);
    assert_eq!(spy_mid.ended.lock().unwrap()[0].completed, 1);
    assert_eq!(spy_slow.ended.lock().unwrap()[0].completed, 2);
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_run_when_the_next_step_is_called_then_it_sees_every_result_of_the_run() {
    let (alpha, _) = echo("alpha");
    let (beta, _) = echo("beta");
    let rig = Rig::new().with_tools(vec![alpha, beta]);
    let model = Model::new(vec![Act::calls(&["alpha", "beta"]), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let seen = history_text(&model.requests()[1]);
    assert!(
        seen.contains("alpha done") && seen.contains("beta done"),
        "{seen}"
    );
    assert!(!history_text(&model.requests()[0]).contains("alpha done"));
}

#[tokio::test(start_paused = true)]
async fn given_a_call_to_an_unknown_tool_when_the_tool_run_ends_then_the_result_is_an_error_naming_the_tool()
 {
    let (alpha, probe) = echo("alpha");
    let rig = Rig::new().with_tools(vec![alpha]);
    let model = Model::new(vec![Act::calls(&["ghost", "alpha"]), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    let conversation = rig.conversation();
    let results = texts_of_results(ai_turns(&conversation)[0]);
    let ghost = results
        .iter()
        .find(|(_, text)| text.contains("ghost"))
        .unwrap();
    assert!(ghost.0, "{results:?}");
    assert!(
        results
            .iter()
            .any(|(is_error, text)| !is_error && text.contains("alpha done"))
    );
    assert_eq!(*probe.starts.lock().unwrap(), 1);
    assert!(completed_tools(&rig.record).contains(&"ghost".to_owned()));
    assert_eq!(update.text, "done");
}

#[tokio::test(start_paused = true)]
async fn given_a_call_that_exceeds_its_timeout_when_the_tool_run_ends_then_the_result_is_an_error_telling_the_timeout()
 {
    let record_rig = Rig::new().with_timeouts(Duration::from_secs(5), Duration::from_secs(2));
    let (stuck, _) = spy("stuck", Plan::Hang, &record_rig.record);
    let (alpha, _) = echo("alpha");
    let rig = record_rig.with_tools(vec![stuck, alpha]);
    let model = Model::new(vec![Act::calls(&["stuck", "alpha"]), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    let update = Driver::result(&node).await;

    let conversation = rig.conversation();
    let results = texts_of_results(ai_turns(&conversation)[0]);
    assert_eq!(results.len(), 2);
    let timed_out = results.iter().find(|(is_error, _)| *is_error).unwrap();
    assert!(
        timed_out.1.contains("timeout") && timed_out.1.contains("2s"),
        "{timed_out:?}"
    );
    assert!(
        results
            .iter()
            .any(|(is_error, text)| !is_error && text.contains("alpha done"))
    );
    assert_eq!(update.text, "done");
}

#[tokio::test(start_paused = true)]
async fn given_a_call_that_exceeds_its_timeout_when_its_result_would_come_later_then_that_result_is_discarded()
 {
    let record_rig = Rig::new().with_timeouts(Duration::from_secs(5), Duration::from_secs(2));
    let (late, spy_late) = spy(
        "late",
        Plan::After(3 * SECOND, "late answer".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![late]);
    let model = Model::new(vec![Act::call("late"), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;
    tokio::time::sleep(10 * SECOND).await;

    assert_eq!(completed_tools(&rig.record), vec!["late"]);
    assert!(spy_late.ended.lock().unwrap().is_empty());
    let conversation = rig.conversation();
    let results = texts_of_results(ai_turns(&conversation)[0]);
    assert_eq!(results.len(), 1);
    assert!(
        results[0].0 && !results[0].1.contains("late answer"),
        "{results:?}"
    );
    assert!(!history_text(&model.requests()[1]).contains("late answer"));
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_faults_when_other_calls_are_running_then_they_are_stopped_and_the_instance_fails()
 {
    let record_rig = Rig::new();
    let dropped = Arc::new(AtomicBool::new(false));
    let (stuck, _) = spy("stuck", Plan::Guarded(dropped.clone()), &record_rig.record);
    let (boom, _) = spy("boom", Plan::Fault, &record_rig.record);
    let rig = record_rig.with_tools(vec![stuck, boom]);
    let model = Model::new(vec![Act::calls(&["stuck", "boom"]), Act::text("never")]);
    let node = node(&rig, plains(&[&model]));

    let failure = Driver::failure(&node).await;

    assert!(failure.contains("the tool broke"), "{failure}");
    assert!(dropped.load(Ordering::SeqCst));
    assert_eq!(count(&rig.record, "complete_result"), 0);
    assert_eq!(discarded_results(&rig.record).len(), 2);
    assert_eq!(model.calls(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_call_when_the_tool_runs_then_it_gets_the_arguments_of_the_call() {
    let record_rig = Rig::new();
    let (alpha, spy_alpha) = spy("alpha", text_plan("alpha"), &record_rig.record);
    let rig = record_rig.with_tools(vec![alpha]);
    let model = Model::new(vec![
        Act::call_with("alpha", "{\"depth\":3}"),
        Act::text("done"),
    ]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let arguments = spy_alpha.arguments.lock().unwrap().clone();
    assert_eq!(arguments.len(), 1);
    assert!(
        arguments[0].contains("\"depth\"") && arguments[0].contains('3'),
        "{arguments:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_tool_that_streams_when_the_call_runs_then_the_fragments_go_to_the_open_result_of_that_call()
 {
    let record_rig = Rig::new();
    let (alpha, _) = spy(
        "alpha",
        Plan::Text("partial then whole".to_owned()),
        &record_rig.record,
    );
    let rig = record_rig.with_tools(vec![alpha]);
    let model = Model::new(vec![Act::call("alpha"), Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    let opened = opened_results(&rig.record);
    assert_eq!(opened.len(), 1);
    assert_eq!(
        result_fragments(&rig.record),
        vec![(opened[0], "partial then whole".to_owned())]
    );
    assert_eq!(completed_result_ids(&rig.record), opened);
    let conversation = rig.conversation();
    assert_eq!(
        texts_of_results(ai_turns(&conversation)[0]).len(),
        1,
        "the copy holds the complete result only"
    );
}
