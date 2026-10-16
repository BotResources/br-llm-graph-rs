//! A durable ReAct agent answered on entry: its outputs come from the
//! completed turn of the call's rank, as the run that wrote it gave them.

use std::sync::Arc;

use serde_json::json;

use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::react_durable_support::*;
use crate::react::round_limit::OnLimit;
use crate::react::test_support::*;
use crate::run::{Outcome, control};
use crate::testkit::{Op, TestStore};

#[tokio::test]
async fn given_a_call_answered_on_entry_when_the_thread_holds_a_later_turn_of_the_agent_then_the_answer_is_the_calls_own()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![text_step("first answer"), text_step("second answer")]);
    let graph = durable(&client, &store).graph().unwrap();
    let host = Host::new();
    let first = opening(&[said("i1", "first")]);

    finished(call_durable(&host, &graph, first.clone(), "", "r1").await);
    let thread = nth_thread(1);
    finished(
        call_durable(
            &host,
            &graph,
            opening(&[said("i2", "second")]),
            thread.as_str(),
            "r2",
        )
        .await,
    );

    let again = finished(call_durable(&host, &graph, first, thread.as_str(), "r1").await);

    assert_eq!(again.str(&key("answer")).unwrap(), "first answer");
    assert_eq!(client.calls(), 2);
}

#[tokio::test]
async fn given_a_turn_the_round_limit_ended_when_the_call_is_answered_on_entry_then_the_flag_is_set_again()
 {
    let store = Arc::new(TestStore::new());
    let client = mock(vec![tool_call_step("c1", "echo", json!({}))]);
    let graph = ReactAgent {
        thread_store: Some(store.clone()),
        inputs: vec![(
            key("log"),
            crate::state::Kind::list(crate::state::Kind::Str),
        )],
        ..limited(
            &client,
            1,
            OnLimit::End {
                node: nid("limit"),
                flag: key("limit_reached"),
            },
        )
    }
    .graph()
    .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "hi")]);
    let (sender, mut channel) = control();
    // The limit node completes the turn, then the run is cancelled before
    // its superstep is applied: the flag it set is lost.
    store.after(Op::EndTurn, 1, move || sender.cancel());
    // One round, then a reply that calls tools again past the limit.
    push(&client, tool_call_step("c2", "echo", json!({})));

    let cancelled = run_in(
        &host,
        &graph,
        start(&graph, question.clone(), ""),
        "r1",
        &mut channel,
    )
    .await;
    assert!(matches!(cancelled.result, Ok(Outcome::Cancelled { .. })));

    let done = finished(call_durable(&host, &graph, question, nth_thread(1).as_str(), "r1").await);

    assert!(done.bool(&key("limit_reached")).unwrap());
    assert_eq!(
        done.str(&key("answer")).unwrap(),
        "Tool round limit reached (1): no final reply."
    );
    assert_eq!(client.calls(), 2);
}
