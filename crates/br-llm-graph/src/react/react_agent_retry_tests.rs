//! The reply retries of a ReAct agent in memory: a reply asked for again
//! with the reminder, never recorded and counted against no round; the
//! attempt its model function reads, the client of a retry kept for the
//! turn; a count read from configuration; a reminder's keys.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use br_llm_clients_core::{Client, ClientError};
use br_llm_clients_mock::Reply;
use br_llm_messages::{StopReason, TurnItem};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Count, Limit};
use crate::react::given::{Model, Reads};
use crate::react::react_agent::ReactAgent;
use crate::react::react_agent_support::*;
use crate::react::round_limit::{OnLimit, RoundLimit};
use crate::react::test_support::*;
use crate::run::{control, run};
use crate::state::{Config, Value};

#[tokio::test]
async fn given_reply_retries_when_the_final_reply_does_not_fit_then_it_is_asked_again_never_recorded_and_the_answer_fits()
 {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        structured_step(json!({ "answer": ["the", "moon"] })),
        structured_step(json!({ "answer": "the moon" })),
    ]);
    let graph = asking_found(&client, Some(Count::Fixed(2)))
        .graph()
        .unwrap();

    let done = finished(call(&graph, question("where")).await);

    assert_eq!(
        done.str(&key("answer")).unwrap(),
        r#"{"answer":"the moon"}"#
    );
    let steps = last_steps(&done);
    assert_eq!(steps.len(), 2, "the tool call and the fitting reply");
    assert_eq!(blocks_of(&steps), [json!({ "answer": "the moon" })]);
    let sent = requests(&client);
    assert_eq!(sent.len(), 3);
    let mut expected = sent[1].messages().to_vec();
    expected.push(reminder_of("`answer` is not a string"));
    assert_eq!(sent[2].messages(), expected.as_slice());
    assert_eq!(sent[2].tools(), sent[1].tools());
    assert_eq!(sent[2].output(), &found_shape());
}

#[tokio::test]
async fn given_a_round_limit_when_replies_are_asked_again_then_they_count_against_no_round() {
    let client = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        structured_step(json!({})),
        structured_step(json!({ "answer": 1 })),
        structured_step(json!({ "answer": "echoed" })),
    ]);
    let graph = ReactAgent {
        round_limit: Some(RoundLimit {
            max_rounds: Limit::Fixed(NonZeroUsize::new(1).unwrap()),
            on_limit: OnLimit::Error,
        }),
        ..asking_found(&client, Some(Count::Fixed(2)))
    }
    .graph()
    .unwrap();

    let done = finished(call(&graph, question("echo")).await);

    assert_eq!(done.str(&key("answer")).unwrap(), r#"{"answer":"echoed"}"#);
    let turn = last_turn(&done);
    let rounds = turn
        .items()
        .iter()
        .filter(|item| matches!(item, TurnItem::ToolResults(_)))
        .count();
    assert_eq!(rounds, 1);
    assert_eq!(
        turn.items().len(),
        3,
        "a tool call, its results, the answer"
    );
}

#[tokio::test]
async fn given_a_model_function_when_a_reply_is_asked_again_then_it_reads_the_attempt_and_its_client_is_kept_for_the_turn()
 {
    let primary = mock(vec![
        tool_call_step("c1", "echo", json!({})),
        structured_step(json!({ "answer": false })),
    ]);
    let fallback = mock(Vec::new());
    for step in [
        tool_call_step("c1", "echo", json!({})),
        structured_step(json!({ "answer": "found" })),
    ] {
        fallback.push_reply(
            Reply::success(step)
                .rename_tool_call_ids("fallback")
                .build()
                .unwrap(),
        );
    }
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (first, second, seen) = (primary.clone(), fallback.clone(), asked.clone());
    let model: Model<ReactAgent> = Model::from_fn(Reads::new(), move |ctx| {
        seen.lock().unwrap().push((ctx.round(), ctx.attempt()));
        let client: Arc<dyn Client> = if ctx.attempt() == 1 {
            Arc::new(first.clone())
        } else {
            Arc::new(second.clone())
        };
        Ok(client)
    });
    let graph = ReactAgent {
        model,
        ..asking_found(&primary, Some(Count::Fixed(1)))
    }
    .graph()
    .unwrap();

    let done = finished(call(&graph, question("find")).await);

    assert_eq!(done.str(&key("answer")).unwrap(), r#"{"answer":"found"}"#);
    assert_eq!(
        *asked.lock().unwrap(),
        [(0, 1), (1, 2)],
        "asked at the turn's first round, then for the retry; the retry's client kept"
    );
    assert_eq!((primary.calls(), fallback.calls()), (2, 2));
}

#[tokio::test]
async fn given_a_plain_agent_when_the_client_refuses_a_reply_then_it_is_asked_again() {
    let refused = ClientError::ReplyUnusable {
        stop_reason: StopReason::MaxTokens,
    };
    let client = mock(Vec::new());
    client.push_reply(Reply::failure(refused.clone()).build().unwrap());
    push(&client, text_step("the moon"));
    let graph = ReactAgent {
        reply_retries: Some(Count::Fixed(1)),
        ..agent(&client)
    }
    .graph()
    .unwrap();

    let done = finished(call(&graph, question("where")).await);

    assert_eq!(done.str(&key("answer")).unwrap(), "the moon");
    assert_eq!(last_steps(&done).len(), 1);
    let sent = requests(&client);
    let mut expected = sent[0].messages().to_vec();
    expected.push(reminder_of(&refused.to_string()));
    assert_eq!(sent[1].messages(), expected.as_slice());
}

#[tokio::test]
async fn given_reply_retries_read_from_a_configuration_key_when_its_value_is_negative_then_the_run_fails_before_any_model_call()
 {
    let client = mock(vec![structured_step(json!({ "answer": "x" }))]);
    let graph = asking_found(&client, Some(Count::Config(key("retries"))))
        .graph()
        .unwrap();
    assert_eq!(
        graph.schema().config.get(&key("retries")),
        Some(&crate::state::Kind::Int)
    );
    let state = graph
        .start_state([(key("opening"), Value::conversation(question("where")))])
        .unwrap();
    let config = Config::new(
        graph.schema(),
        BTreeMap::from([
            (key("base"), Value::str("You are a helpful agent.")),
            (key("retries"), Value::int(-2)),
        ]),
    )
    .unwrap();
    let (_sender, mut channel) = control();

    let failure = run(&graph, &config, state, None, &ctx(), &mut channel)
        .await
        .err()
        .unwrap();

    assert!(matches!(
        failure.error,
        GraphError::CountNegative { value: -2, .. }
    ));
    assert_eq!(client.calls(), 0);
}

#[test]
fn given_a_reminder_reading_a_state_key_when_the_agent_is_built_then_the_key_is_an_input_of_its_graph()
 {
    let mut reads = Reads::new();
    let tone = reads.state::<String>("tone").unwrap();
    let graph = ReactAgent {
        reminder: Some(crate::react::given::Prompt::from_fn(reads, move |ctx| {
            Ok(format!("{} {{reason}}", ctx.get(&tone)?))
        })),
        ..agent(&mock(Vec::new()))
    }
    .graph()
    .unwrap();
    assert!(graph.signature().inputs.contains_key(&key("tone")));
}
