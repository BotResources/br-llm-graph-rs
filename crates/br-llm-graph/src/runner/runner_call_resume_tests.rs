//! A runner call across attempts: a resumed run keeps its run id, so it
//! gives the keys it gave before. A map resumed after a cancel calls the
//! port only for the items that did not finish, under the keys they had; a
//! node its port failed calls it again under its key.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::graph::ItemFailure;
use crate::observe::{NoopObserver, Observer};
use crate::origin::Origin;
use crate::run::{ControlSender, Outcome, PendingEntry, control, run};
use crate::runner::test_support::*;
use crate::testkit::{Scripted, TestRunners, text_block};

/// Cancels the run once `after` map items have recorded their answers.
struct CancelAfter {
    after: usize,
    seen: Mutex<usize>,
    sender: ControlSender,
}

impl Observer for CancelAfter {
    fn recorded(&self, _origin: &Origin, entry: &PendingEntry) {
        if entry.witness.is_none() {
            return;
        }
        let mut seen = self.seen.lock().unwrap();
        *seen += 1;
        if *seen == self.after {
            self.sender.cancel();
        }
    }
}

/// A port whose `documentalist` answers with the analysis of the document
/// it is asked about.
fn analysing() -> Arc<TestRunners> {
    let port = Arc::new(TestRunners::new());
    port.script("documentalist", |request| {
        let document = request
            .arguments
            .get("document")
            .and_then(|value| value.as_str())
            .unwrap_or("nothing");
        Scripted::Answer(vec![text_block(&format!("analysis of {document}"))])
    });
    port
}

#[tokio::test(flavor = "current_thread")]
async fn given_a_map_cancelled_while_one_call_waits_when_resumed_then_only_that_item_is_called_again_under_its_key()
 {
    let port = analysing();
    let waiting = call_key("run-1", "each[1]", None, &about("b"));
    port.hold_call(waiting.clone());
    let graph = mapped(ask_about_document(&port), None, ItemFailure::Finish);
    let (sender, mut channel) = control();
    let observer = Arc::new(CancelAfter {
        after: 2,
        seen: Mutex::new(0),
        sender,
    });

    let outcome = run_with(
        &graph,
        state_with("", &["a", "b", "c"]),
        &ctx_heard("run-1", observer),
        &mut channel,
    )
    .await;
    let Ok(Outcome::Cancelled { checkpoint }) = outcome else {
        panic!("expected cancelled");
    };
    assert_eq!(
        port.dropped(),
        std::slice::from_ref(&waiting),
        "the waiting call is dropped"
    );
    let mut first = port.keys();
    first.sort_by_key(|key| key.origin.occurrence.to_string());
    assert_eq!(
        first,
        [("each[0]", "a"), ("each[1]", "b"), ("each[2]", "c")].map(|(item, document)| call_key(
            "run-1",
            item,
            None,
            &about(document)
        ))
    );
    assert!(texts(&checkpoint.state, "answers").is_empty());

    port.release_call(&waiting);
    let (_sender, mut channel) = control();
    let resumed = ctx_heard("run-1", Arc::new(NoopObserver)).with_pending(checkpoint.pending);
    let state = finished(
        run(
            &graph,
            &config(),
            checkpoint.state,
            Some(checkpoint.cursor),
            &resumed,
            &mut channel,
        )
        .await,
    );

    assert_eq!(
        port.keys()[first.len()..],
        [waiting],
        "the finished items are not called again, the waiting one under its key"
    );
    assert_eq!(
        texts(&state, "answers"),
        ["analysis of a", "analysis of b", "analysis of c"]
    );
    assert_eq!(port.dropped().len(), 1);
}

#[tokio::test]
async fn given_a_node_its_port_failed_when_the_run_is_resumed_from_its_checkpoint_then_the_port_is_called_under_the_same_key()
 {
    let port = Arc::new(TestRunners::new());
    let calls = AtomicUsize::new(0);
    port.script("documentalist", move |_| {
        match calls.fetch_add(1, Ordering::SeqCst) {
            0 => Scripted::Break("the bus is down".to_owned()),
            _ => Scripted::Answer(vec![text_block("an answer")]),
        }
    });
    let graph = alone(ask(&port, set_answer()));
    let failure = run_in(&graph, state_with("", &[]), &ctx("run-1"))
        .await
        .err()
        .unwrap();

    let (_sender, mut channel) = control();
    let state = finished(
        run(
            &graph,
            &config(),
            failure.checkpoint.state,
            Some(failure.checkpoint.cursor),
            &ctx("run-1").with_pending(failure.checkpoint.pending),
            &mut channel,
        )
        .await,
    );

    assert_eq!(state.str(&key("answer")).unwrap(), "an answer");
    assert_eq!(
        port.keys(),
        [
            call_key("run-1", "ask", None, &in_french()),
            call_key("run-1", "ask", None, &in_french())
        ]
    );
}
