use std::sync::Arc;

use super::support::*;
use crate::{GraphError, NoopObserver, Outcome, State, channel, run};

const FAILING: [&str; 2] = ["item07", "item12"];

async fn uninterrupted() -> State {
    let probe = Arc::new(Probe::default());
    let graph = caller(per_item(&probe)).unwrap();
    let (_sender, mut inbox) = channel();
    let ctx = context(Arc::new(NoopObserver));
    match run(
        &graph,
        &config(&graph),
        start(&graph, 20),
        None,
        &ctx,
        &mut inbox,
    )
    .await
    {
        Ok(Outcome::Finished { state, .. }) => state,
        _ => panic!("expected finished"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn given_failing_items_that_propagate_when_resumed_then_finished_items_do_not_run_again() {
    let probe = Probe::failing(&FAILING);
    let graph = caller(per_item(&probe)).unwrap();
    let (_sender, mut inbox) = channel();
    let ctx = context(Arc::new(NoopObserver));
    let failure = run(
        &graph,
        &config(&graph),
        start(&graph, 20),
        None,
        &ctx,
        &mut inbox,
    )
    .await
    .err()
    .unwrap();
    assert!(matches!(failure.error, GraphError::NodeFailed { .. }));
    let checkpoint = failure.checkpoint;
    let finished: Vec<String> = checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    let expected: Vec<String> = (0..20)
        .filter(|n| *n != 7 && *n != 12)
        .map(|n| format!("each[{n}]"))
        .collect();
    let mut sorted = finished.clone();
    sorted.sort_by_key(|occurrence| number(occurrence));
    assert_eq!(sorted, expected);
    assert!(checkpoint.state.list(&key("analyses")).unwrap().is_empty());

    probe.heal();
    let (_sender, mut inbox) = channel();
    let resumed = context(Arc::new(NoopObserver)).with_pending(checkpoint.pending);
    let outcome = run(
        &graph,
        &config(&graph),
        checkpoint.state,
        Some(checkpoint.cursor),
        &resumed,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("expected finished");
    };
    for (item, calls) in probe.prepared() {
        let expected = if FAILING.contains(&item.as_str()) {
            2
        } else {
            1
        };
        assert_eq!(calls, expected, "{item} was prepared {calls} times");
    }
    assert_eq!(state, uninterrupted().await);
}

#[tokio::test(flavor = "current_thread")]
async fn given_failing_items_that_are_captured_when_mapped_then_the_run_completes_and_lists_them_in_item_order()
 {
    let probe = Probe::failing(&FAILING);
    let graph = caller(capture_failures(per_item(&probe))).unwrap();
    let (_sender, mut inbox) = channel();
    let ctx = context(Arc::new(NoopObserver));
    let outcome = run(
        &graph,
        &config(&graph),
        start(&graph, 20),
        None,
        &ctx,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Finished { state, .. }) = outcome else {
        panic!("a captured failure must not fail the run");
    };
    assert_eq!(texts(&state, "failed_items"), FAILING.to_vec());
    let failures = texts(&state, "failures");
    assert_eq!(failures.len(), 2);
    assert!(failures[0].ends_with("critic unavailable for ITEM07"));
    assert!(failures[1].ends_with("critic unavailable for ITEM12"));
    let analyses: Vec<String> = item_names(20)
        .iter()
        .filter(|name| !FAILING.contains(&name.as_str()))
        .map(|name| expected_analysis(name))
        .collect();
    assert_eq!(texts(&state, "analyses"), analyses);
    assert_eq!(state.list(&key("attempts")).unwrap().len(), 18);
}
