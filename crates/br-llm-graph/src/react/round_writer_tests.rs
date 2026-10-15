use std::sync::Arc;
use std::sync::Mutex;

use br_llm_messages::{Text, ToolCallId, ToolName, ToolResult, ToolResultBlock};

use super::*;

fn result(id: &str) -> ToolResult {
    ToolResult::new(
        ToolCallId::new(id).unwrap(),
        ToolName::new("echo").unwrap(),
        vec![ToolResultBlock::text(Text::new("echoed").unwrap())],
        false,
    )
}

fn ids(results: &[ToolResult]) -> Vec<String> {
    results
        .iter()
        .map(|result| result.tool_call_id.as_str().to_owned())
        .collect()
}

/// A write that keeps what it was given and gives `outcome`.
fn writing(
    written: &Arc<Mutex<Vec<Vec<String>>>>,
    outcome: Result<(), &'static str>,
) -> impl FnOnce(Vec<ToolResult>) -> std::future::Ready<Result<(), GraphError>> + use<> {
    let written = written.clone();
    move |all| {
        written.lock().unwrap().push(ids(&all));
        std::future::ready(outcome.map_err(|message| GraphError::Thread {
            source: Arc::from(Box::<dyn std::error::Error + Send + Sync>::from(message)),
        }))
    }
}

#[tokio::test]
async fn given_two_tool_nodes_when_both_deposit_then_the_round_is_written_once_in_node_order() {
    let writers = Arc::new(RoundWriters::default());
    let state = State::empty();
    let written = Arc::default();
    let first = writers.seat(&state, 2, 0);
    let second = writers.seat(&state, 2, 1);
    // The second node finishes first: the round still keeps node order.
    let (later, earlier) = futures_util::join!(
        second.write(vec![result("c2")], writing(&written, Ok(()))),
        first.write(vec![result("c1")], writing(&written, Ok(()))),
    );
    assert!(later.is_ok() && earlier.is_ok());
    assert_eq!(*written.lock().unwrap(), [["c1", "c2"]]);
    assert!(writers.lock().is_empty(), "the round is forgotten");
}

#[tokio::test]
async fn given_a_tool_node_that_fails_when_the_others_deposit_then_nothing_is_written_and_none_waits()
 {
    let writers = Arc::new(RoundWriters::default());
    let state = State::empty();
    let written = Arc::default();
    let failed = writers.seat(&state, 2, 0);
    let other = writers.seat(&state, 2, 1);
    drop(failed);
    let outcome = other
        .write(vec![result("c2")], writing(&written, Ok(())))
        .await;
    assert!(outcome.is_ok());
    assert!(written.lock().unwrap().is_empty());
}

#[tokio::test]
async fn given_a_store_that_refuses_the_round_when_it_is_written_then_every_tool_node_fails_with_its_error()
 {
    let writers = Arc::new(RoundWriters::default());
    let state = State::empty();
    let written = Arc::default();
    let first = writers.seat(&state, 2, 0);
    let second = writers.seat(&state, 2, 1);
    let (one, two) = futures_util::join!(
        first.write(vec![result("c1")], writing(&written, Err("refused"))),
        second.write(vec![result("c2")], writing(&written, Err("refused"))),
    );
    for outcome in [one, two] {
        assert!(
            matches!(outcome, Err(GraphError::Thread { ref source }) if source.to_string() == "refused")
        );
    }
    assert_eq!(written.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn given_two_supersteps_when_their_tool_nodes_deposit_then_each_round_is_written_apart() {
    let writers = Arc::new(RoundWriters::default());
    let (one, two) = (State::empty(), State::empty());
    let written = Arc::default();
    let (a, b) = (writers.seat(&one, 1, 0), writers.seat(&two, 1, 0));
    let (first, second) = futures_util::join!(
        a.write(vec![result("c1")], writing(&written, Ok(()))),
        b.write(vec![result("c2")], writing(&written, Ok(()))),
    );
    assert!(first.is_ok() && second.is_ok());
    assert_eq!(*written.lock().unwrap(), [["c1"], ["c2"]]);
}
