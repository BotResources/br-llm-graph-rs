//! The reply retries of an `LlmCall` as the runner gives them: the attempt
//! its model function reads, its reminder, a count read from configuration,
//! and the build refusals of their keys.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use br_llm_clients_core::Client;
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Always, Count, GraphBuilder, Target};
use crate::react::given::{Model, Prompt, Reads};
use crate::react::llm_call::LlmCall;
use crate::react::llm_call_support::*;
use crate::react::test_support::{key, mock, requests, returned, structured_step};
use crate::run::{control, run};
use crate::state::{Config, Kind, Value};
use crate::testkit::refusal;
use crate::value::EndLabel;

#[tokio::test]
async fn given_a_model_function_when_a_reply_is_asked_again_then_it_reads_the_attempt_and_may_give_another_client()
 {
    let first = mock(vec![structured_step(json!({ "subject": "tides" }))]);
    let second = mock(vec![structured_step(
        json!({ "subject": "tides", "count": 2 }),
    )]);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (primary, fallback, attempts) = (first.clone(), second.clone(), seen.clone());
    let model: Model<LlmCall> = Model::from_fn(Reads::new(), move |ctx| {
        attempts.lock().unwrap().push(ctx.attempt());
        let client: Arc<dyn Client> = if ctx.attempt() == 1 {
            Arc::new(primary.clone())
        } else {
            Arc::new(fallback.clone())
        };
        Ok(client)
    });
    let call = LlmCall {
        model,
        ..asking_facts(&first, Some(Count::Fixed(1)))
    };

    let answer = summary_of(call).await.unwrap();

    assert_eq!(answer, r#"{"subject":"tides","count":2}"#);
    assert_eq!(*seen.lock().unwrap(), [1, 2]);
    assert_eq!((first.calls(), second.calls()), (1, 1));
}

#[tokio::test]
async fn given_a_reminder_when_a_reply_is_asked_again_then_its_text_holds_the_reason_where_it_says()
{
    let client = mock(vec![
        structured_step(json!({ "count": 1 })),
        structured_step(json!({ "subject": "tides", "count": 1 })),
    ]);
    let call = LlmCall {
        reminder: Some(Prompt::from_fn(Reads::new(), |ctx| {
            Ok(format!("Attempt {}: {{reason}}.", ctx.attempt()))
        })),
        ..asking_facts(&client, Some(Count::Fixed(1)))
    };

    summary_of(call).await.unwrap();

    assert_eq!(
        requests(&client)[1].messages(),
        [
            user_text("doc"),
            user_text("Attempt 2: `subject` is missing or null.")
        ]
    );
}

#[tokio::test]
async fn given_a_reminder_that_resolves_blank_when_a_reply_is_asked_again_then_the_request_is_sent_alone()
 {
    let client = mock(vec![
        structured_step(json!({ "count": 1 })),
        structured_step(json!({ "subject": "tides", "count": 1 })),
    ]);
    let call = LlmCall {
        reminder: Some(Prompt::text(" ")),
        ..asking_facts(&client, Some(Count::Fixed(1)))
    };

    summary_of(call).await.unwrap();

    let sent = requests(&client);
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[1].messages(), sent[0].messages());
}

/// Runs `call` alone in a graph that also declares the int configuration
/// key `retries`, set to `value`: the summary it wrote, or its error.
async fn with_retries_key(call: LlmCall, value: i64) -> Result<String, GraphError> {
    let mut schema = schema();
    schema.config.insert(key("retries"), Kind::Int);
    let graph = GraphBuilder::new(schema.clone())
        .entry(nid("call"))
        .node(nid("call"), call)
        .edge(
            nid("call"),
            Always(Target::End(EndLabel::new("done").unwrap())),
        )
        .build()
        .unwrap();
    let config = Config::new(
        &schema,
        BTreeMap::from([
            (key("style"), Value::str("Be terse.")),
            (key("retries"), Value::int(value)),
        ]),
    )
    .unwrap();
    let state = state_with("doc", &[]).extended(&schema);
    let (_sender, mut channel) = control();
    run(&graph, &config, state, None, &ctx(), &mut channel)
        .await
        .map(|outcome| {
            finished(Ok(outcome))
                .str(&key("summary"))
                .unwrap()
                .to_owned()
        })
        .map_err(|failure| failure.error)
}

#[tokio::test]
async fn given_reply_retries_read_from_a_configuration_key_when_the_call_runs_then_its_value_counts()
 {
    let client = mock(vec![
        structured_step(json!({ "subject": "tides" })),
        structured_step(json!({ "subject": "tides", "count": 3 })),
    ]);
    let retries = Count::Config(key("retries"));

    let answer = with_retries_key(asking_facts(&client, Some(retries.clone())), 1)
        .await
        .unwrap();
    assert_eq!(answer, r#"{"subject":"tides","count":3}"#);

    let unused = mock(Vec::new());
    let error = with_retries_key(asking_facts(&unused, Some(retries)), -1)
        .await
        .unwrap_err();
    assert!(matches!(
        returned(&error),
        GraphError::CountNegative { value: -1, .. }
    ));
    assert_eq!(unused.calls(), 0, "the model is not called");
}

#[test]
fn given_reply_retries_on_a_key_that_is_not_an_int_configuration_key_when_built_then_count_key_mismatch()
 {
    let call = asking_facts(&mock(Vec::new()), Some(Count::Config(key("style"))));
    let refused = refusal(
        GraphBuilder::new(schema())
            .entry(nid("call"))
            .node(nid("call"), call)
            .edge(
                nid("call"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build(),
    );
    assert!(matches!(refused, GraphError::CountKeyMismatch { key } if key.as_str() == "style"));
}

#[test]
fn given_a_reminder_reading_a_key_the_graph_lacks_when_built_then_unknown_key() {
    let mut reads = Reads::new();
    reads.state::<String>("absent").unwrap();
    let call = LlmCall {
        reminder: Some(Prompt::from_fn(reads, |_| Ok("again".to_owned()))),
        ..summarize(&mock(Vec::new()), set_summary())
    };
    let refused = refusal(
        GraphBuilder::new(schema())
            .entry(nid("call"))
            .node(nid("call"), call)
            .edge(
                nid("call"),
                Always(Target::End(EndLabel::new("done").unwrap())),
            )
            .build(),
    );
    assert!(matches!(refused, GraphError::UnknownKey { key } if key.as_str() == "absent"));
}
