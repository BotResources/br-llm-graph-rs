use std::sync::{Arc, Mutex};

use super::support::*;
use crate::{NoopObserver, Observer, Origin, Outcome, PendingEntry, Sender, channel, run};

/// Cancels the run once `after` items have recorded their result.
struct CancelAfter {
    after: usize,
    seen: Mutex<usize>,
    sender: Sender,
}

impl Observer for CancelAfter {
    fn recorded(&self, _origin: &Origin, _entry: &PendingEntry) {
        let mut seen = self.seen.lock().unwrap();
        *seen += 1;
        if *seen == self.after {
            self.sender.cancel();
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_cancel_in_the_middle_of_the_map_when_resumed_then_finished_items_do_not_run_again()
{
    let probe = Arc::new(Probe::default());
    let graph = caller(per_item(&probe)).unwrap();
    let (sender, mut inbox) = channel();
    let observer = Arc::new(CancelAfter {
        after: 6,
        seen: Mutex::new(0),
        sender,
    });
    let ctx = context(observer);
    let outcome = run(
        &graph,
        &config(&graph),
        start(&graph, 20),
        None,
        &ctx,
        &mut inbox,
    )
    .await;
    let Ok(Outcome::Cancelled { checkpoint }) = outcome else {
        panic!("expected cancelled");
    };
    let finished: Vec<String> = checkpoint
        .pending
        .iter()
        .map(|(occurrence, _)| occurrence.to_string())
        .collect();
    assert!(finished.len() >= 6 && finished.len() < 20, "{finished:?}");
    assert!(checkpoint.state.list(&key("analyses")).unwrap().is_empty());
    let started_before = probe.prepared();

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
    let names = item_names(20);
    let analyses: Vec<String> = names.iter().map(|name| expected_analysis(name)).collect();
    assert_eq!(texts(&state, "analyses"), analyses);
    for (position, name) in names.iter().enumerate() {
        let calls = probe.prepared().get(name).copied().unwrap_or(0);
        let was_finished = finished.contains(&format!("each[{position}]"));
        let was_started = started_before.contains_key(name);
        let expected = match (was_finished, was_started) {
            (true, _) => 1,
            (false, true) => 2,
            (false, false) => 1,
        };
        assert_eq!(calls, expected, "{name} was prepared {calls} times");
    }
}
