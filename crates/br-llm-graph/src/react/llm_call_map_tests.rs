//! `LlmCall` as the body of a map: one model call per item, the answers
//! appended in item order, at most the map's concurrency limit at once.

use br_llm_clients_core::ReasoningLevel;
use br_llm_clients_mock::{Gate, Reply, ReplySource, Rule, StateKind};

use crate::error::GraphError;
use crate::graph::{Always, CheckSite, GraphBuilder, Map, Node, Target};
use crate::react::llm_call_support::*;
use crate::react::test_support::{key, mock, mock_at, text_step};
use crate::state::Value;
use crate::value::EndLabel;

#[tokio::test]
async fn given_an_append_call_as_a_map_body_when_run_then_one_call_per_item_appended_in_item_order()
{
    // `beta` is held at a gate until the other items have answered.
    let gate = Gate::new();
    let held = Reply::success(text_step("summary of beta"))
        .gate_at(0, &gate)
        .build()
        .unwrap();
    let client = mock_at(ReasoningLevel::Medium);
    client.push_rule(Rule::new(|call| asked(call.request()) == "beta", held));
    client.set_fallback(echoing());
    let graph = mapped(summarize(&client, append_summaries()), None);
    let release = async {
        gate.wait_for_waiting(1).await;
        client
            .wait_for_state(StateKind::Succeeded, 2)
            .unwrap()
            .await;
        gate.open();
    };

    let ctx = ctx();
    let (result, ()) = futures_util::join!(
        run_in(&graph, state_with("", &["alpha", "beta", "gamma"]), &ctx),
        release
    );

    let state = finished(result);
    assert_eq!(
        state.list(&key("summaries")).unwrap(),
        &[
            Value::str("summary of alpha"),
            Value::str("summary of beta"),
            Value::str("summary of gamma"),
        ]
    );
    assert_eq!(client.calls(), 3);
}

#[tokio::test]
async fn given_a_map_concurrency_limit_when_the_calls_run_then_at_most_that_many_at_once() {
    let gate = Gate::new();
    let client = mock_at(ReasoningLevel::Medium);
    let held = gate.clone();
    client.set_fallback(ReplySource::function(move |call| {
        Reply::success(text_step(&format!("summary of {}", asked(call.request()))))
            .gate_at(0, &held)
            .build()
    }));
    let graph = mapped(summarize(&client, append_summaries()), Some(2));
    let check = async {
        gate.wait_for_waiting(2).await;
        let started = client.calls();
        gate.open();
        started
    };

    let ctx = ctx();
    let (result, started) = futures_util::join!(
        run_in(&graph, state_with("", &["a", "b", "c", "d"]), &ctx),
        check
    );

    assert_eq!(started, 2, "no third call begins while two are held");
    let state = finished(result);
    assert_eq!(client.calls(), 4);
    assert_eq!(
        state.list(&key("summaries")).unwrap(),
        &["a", "b", "c", "d"].map(|item| Value::str(format!("summary of {item}")))
    );
}

#[test]
fn given_a_set_output_as_a_map_body_when_built_then_map_body_set() {
    let client = mock(Vec::new());
    let map = Map {
        list: key("docs"),
        item: key("doc"),
        body: Box::new(summarize(&client, set_summary())),
        max_concurrency: None,
        on_item_failure: Default::default(),
    };
    let built = GraphBuilder::new(schema())
        .entry(nid("each"))
        .map(nid("each"), map)
        .edge(
            nid("each"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build();
    assert!(matches!(
        crate::testkit::refusal(built),
        GraphError::MapBodySet { key: found } if found == key("summary")
    ));
    // Positive control: the same call as a node of the graph.
    assert!(
        summarize(&client, set_summary())
            .check(&schema(), CheckSite::Graph)
            .is_ok()
    );
}
