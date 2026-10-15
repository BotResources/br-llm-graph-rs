//! `LlmCall` built from functions of its context: a model chosen from the
//! state, a history given whole, the keys a function may read, and a
//! function's error.

use std::sync::Arc;

use br_llm_clients_core::{BuildError, Client};
use br_llm_messages::WireMessage;

use crate::error::{GraphError, NodeFault};
use crate::graph::Context;
use crate::observe::Observer;
use crate::react::given::{History, Model, Prompt, Reads};
use crate::react::llm_call::{LlmCall, LlmRequest};
use crate::react::llm_call_support::*;
use crate::react::test_support::{
    StreamRecorder, ckey, key, mock, requests, returned, skey, system_of, text_step,
};
use crate::state::{ConfigKey, StateKey};
use crate::testkit::SeqIds;

/// The error a runner's function returns.
#[derive(Debug, PartialEq)]
struct NoModelFor(String);

impl std::fmt::Display for NoModelFor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no model for {}", self.0)
    }
}

impl std::error::Error for NoModelFor {}

#[tokio::test]
async fn given_a_model_function_when_the_call_runs_then_it_calls_the_client_the_state_chooses() {
    let short = mock(vec![text_step("from short")]);
    let long = mock(vec![text_step("from long")]);
    let clients: [Arc<dyn Client>; 2] = [Arc::new(short.clone()), Arc::new(long.clone())];
    let mut reads = Reads::new();
    let doc: StateKey<String> = reads.state("doc").unwrap();
    let call = LlmCall {
        model: Model::from_fn(reads, move |ctx| {
            Ok(clients[usize::from(ctx.get(&doc)?.len() > 10)].clone())
        }),
        ..summarize(&short, set_summary())
    };
    let graph = single(call);

    let state = finished(run_in(&graph, state_with("A long note on tides.", &[]), &ctx()).await);

    assert_eq!(state.str(&key("summary")).unwrap(), "from long");
    assert_eq!((short.calls(), long.calls()), (0, 1));
}

#[tokio::test]
async fn given_a_function_that_fails_when_the_call_runs_then_the_node_fails_with_its_error_as_returned()
 {
    let client = mock(vec![text_step("unused")]);
    let call = LlmCall {
        model: Model::from_fn(Reads::new(), |_| Err(NoModelFor("tides".to_owned()).into())),
        ..summarize(&client, set_summary())
    };

    let failure = run_in(&single(call), state_with("doc", &[]), &ctx())
        .await
        .err()
        .unwrap();

    match &failure.error {
        GraphError::NodeFailed {
            source: NodeFault::Returned(error),
            ..
        } => assert_eq!(
            error.downcast_ref::<NoModelFor>(),
            Some(&NoModelFor("tides".to_owned()))
        ),
        other => panic!("expected the function's error, got {other}"),
    }
    assert_eq!(client.calls(), 0);
}

#[tokio::test]
async fn given_a_function_reading_a_key_it_did_not_declare_when_the_call_runs_then_unknown_key() {
    let client = mock(vec![text_step("unused")]);
    let doc: StateKey<String> = skey("doc");
    let call = LlmCall {
        system: Prompt::from_fn(Reads::new(), move |ctx| Ok(ctx.get(&doc)?.to_owned())),
        ..summarize(&client, set_summary())
    };

    let failure = run_in(&single(call), state_with("doc", &[]), &ctx())
        .await
        .err()
        .unwrap();

    assert!(
        matches!(
            returned(&failure.error),
            GraphError::UnknownKey { key } if key.as_str() == "doc"
        ),
        "the state holds `doc`, the function did not declare it"
    );
    assert_eq!(client.calls(), 0);
}

#[tokio::test]
async fn given_a_function_reading_a_configuration_key_it_did_not_declare_when_the_call_runs_then_unknown_key()
 {
    let client = mock(vec![text_step("unused")]);
    let style: ConfigKey<String> = ckey("style");
    // It declares the state key `doc`, not the configuration key `style`.
    let mut reads = Reads::new();
    reads.read_state(&skey::<String>("doc"));
    let call = LlmCall {
        system: Prompt::from_fn(reads, move |ctx| Ok(ctx.config(&style)?.to_owned())),
        ..summarize(&client, set_summary())
    };

    let failure = run_in(&single(call), state_with("doc", &[]), &ctx())
        .await
        .err()
        .unwrap();

    assert!(matches!(
        returned(&failure.error),
        GraphError::UnknownKey { key } if key.as_str() == "style"
    ));
    assert_eq!(client.calls(), 0);
}

#[tokio::test]
async fn given_a_history_request_when_the_call_runs_then_its_messages_are_sent_and_its_events_carry_the_output_key()
 {
    let client = mock(vec![text_step("answer")]);
    let mut reads = Reads::new();
    let docs: StateKey<Vec<String>> = reads.state("docs").unwrap();
    let call = LlmCall {
        request: LlmRequest::History(History::from_fn(reads, move |ctx| {
            Ok(ctx
                .get(&docs)?
                .into_iter()
                .map(|doc| user_text(&format!("Read: {doc}")))
                .collect())
        })),
        ..summarize(&client, set_summary())
    };
    let recorder = Arc::new(StreamRecorder::default());
    let observer: Arc<dyn Observer> = recorder.clone();
    let ctx = Context::new(observer, Arc::new(SeqIds::new()));

    finished(run_in(&single(call), state_with("doc", &["one", "two"]), &ctx).await);

    let request = &requests(&client)[0];
    assert_eq!(
        request.messages(),
        [user_text("Read: one"), user_text("Read: two")]
    );
    assert_eq!(system_of(request).as_deref(), Some("Be terse."));
    assert!(!recorder.events().is_empty());
    assert!(
        recorder.events().iter().all(|(_, key, _)| key == "summary"),
        "a request that is not a conversation key streams under the output key"
    );
}

#[tokio::test]
async fn given_a_history_request_that_gives_nothing_when_the_call_runs_then_the_node_fails_unsent()
{
    let client = mock(vec![text_step("unused")]);
    let call = LlmCall {
        request: LlmRequest::History(History::value(Vec::<WireMessage>::new())),
        ..summarize(&client, set_summary())
    };

    let failure = run_in(&single(call), state_with("doc", &[]), &ctx())
        .await
        .err()
        .unwrap();

    assert!(matches!(
        returned(&failure.error),
        GraphError::Request(BuildError::NoMessage)
    ));
    assert_eq!(client.calls(), 0);
}
