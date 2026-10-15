//! `LlmCall` as a node of a graph: the request it sends, the answer it
//! writes through its output, and how it fails.

use std::sync::Arc;

use br_llm_clients_core::{
    ClientError, OutputMode, Parameter, ParameterKind, Parameters, StructuredShape,
};
use br_llm_clients_mock::{Chunking, derive_events};
use br_llm_messages::{AssistantBlock, MessageError, Step, StopReason, Text};
use serde_json::json;

use crate::error::GraphError;
use crate::graph::{Context, Node, Output};
use crate::observe::Observer;
use crate::react::llm_call::{LlmCall, LlmRequest};
use crate::react::llm_call_support::*;
use crate::react::source::Source;
use crate::react::test_support::{
    StreamRecorder, author, failing, key, mock, requests, returned, structured_step, system_of,
    text_step,
};
use crate::state::Value;
use crate::testkit::SeqIds;

#[tokio::test]
async fn given_a_text_request_when_the_call_runs_then_one_user_message_and_the_answer_is_set() {
    let client = mock(vec![text_step("Tides follow the moon.")]);
    let graph = single(summarize(&client, Output::Set(key("summary"))));

    let state = finished(run_in(&graph, state_with("A note on tides.", &[]), &ctx()).await);

    assert_eq!(
        state.str(&key("summary")).unwrap(),
        "Tides follow the moon."
    );
    let request = &requests(&client)[0];
    assert_eq!(system_of(request).as_deref(), Some("Be terse."));
    assert_eq!(request.messages(), [user_text("A note on tides.")]);
    assert!(request.tools().is_empty(), "the request declares no tools");
    assert_eq!(request.output(), &OutputMode::Plain);
}

#[tokio::test]
async fn given_an_append_output_when_the_call_runs_then_the_answer_is_appended() {
    let client = mock(vec![text_step("first"), text_step("second")]);
    let graph = single(summarize(&client, Output::Append(key("summaries"))));

    let state = finished(run_in(&graph, state_with("doc", &[]), &ctx()).await);
    let state = finished(run_in(&graph, state, &ctx()).await);

    assert_eq!(
        state.list(&key("summaries")).unwrap(),
        &[Value::str("first"), Value::str("second")]
    );
}

#[tokio::test]
async fn given_a_call_when_it_runs_then_nothing_but_its_output_is_written() {
    let client = mock(vec![text_step("answer")]);
    let call = LlmCall {
        request: LlmRequest::Conversation {
            key: key("chat"),
            author: author(),
        },
        ..summarize(&client, Output::Set(key("summary")))
    };
    let before = state_with("doc", &[]);

    let updates = call
        .run(&before, &config(), &ctx().for_node(&nid("call")))
        .await
        .unwrap();

    assert_eq!(
        updates,
        vec![crate::update::Update::Set {
            key: key("summary"),
            value: Value::str("answer"),
        }],
        "the call keeps no conversation"
    );
}

#[tokio::test]
async fn given_a_structured_reply_when_the_call_runs_then_the_answer_is_its_json_text() {
    let shape = StructuredShape::new(
        "facts",
        None,
        Parameters::new(vec![
            Parameter::new("subject", "", true, ParameterKind::String).unwrap(),
            Parameter::new("count", "", true, ParameterKind::Integer).unwrap(),
        ])
        .unwrap(),
    )
    .unwrap();
    let client = mock(vec![structured_step(
        json!({ "subject": "tides", "count": 2 }),
    )]);
    let call = LlmCall {
        output_mode: OutputMode::Structured(shape.clone()),
        ..summarize(&client, Output::Set(key("summary")))
    };

    let state = finished(run_in(&single(call), state_with("doc", &[]), &ctx()).await);

    assert_eq!(
        state.str(&key("summary")).unwrap(),
        r#"{"subject":"tides","count":2}"#
    );
    assert_eq!(
        requests(&client)[0].output(),
        &OutputMode::Structured(shape)
    );
}

#[tokio::test]
async fn given_a_reply_of_several_text_blocks_when_the_call_runs_then_they_are_joined_verbatim() {
    let several = Step::new(
        vec![
            AssistantBlock::Text {
                text: Text::new("one ").unwrap(),
            },
            AssistantBlock::Text {
                text: Text::new("two").unwrap(),
            },
        ],
        StopReason::EndTurn,
        None,
        None,
    )
    .unwrap();
    let client = mock(vec![several]);
    let graph = single(summarize(&client, Output::Set(key("summary"))));

    let state = finished(run_in(&graph, state_with("doc", &[]), &ctx()).await);

    assert_eq!(state.str(&key("summary")).unwrap(), "one two");
}

#[tokio::test]
async fn given_a_request_that_resolves_blank_when_the_call_runs_then_the_node_fails_unsent() {
    for blank in ["", "  \n"] {
        let client = mock(vec![text_step("unused")]);
        let graph = single(summarize(&client, Output::Set(key("summary"))));

        let failure = run_in(&graph, state_with(blank, &[]), &ctx())
            .await
            .err()
            .unwrap();

        assert!(
            matches!(
                returned(&failure.error),
                GraphError::Message(MessageError::Blank { field: "text" })
            ),
            "{blank:?}: {}",
            failure.error
        );
        assert_eq!(client.calls(), 0, "a blank request reaches no client");
        assert_eq!(failure.checkpoint.state.str(&key("summary")).unwrap(), "");
    }
}

#[tokio::test]
async fn given_a_system_source_that_resolves_blank_when_the_call_runs_then_no_system_text() {
    let client = mock(vec![text_step("answer")]);
    let call = LlmCall {
        system: Source::Const(" ".to_owned()),
        ..summarize(&client, Output::Set(key("summary")))
    };

    finished(run_in(&single(call), state_with("doc", &[]), &ctx()).await);

    assert_eq!(system_of(&requests(&client)[0]), None);
}

#[tokio::test]
async fn given_a_computed_request_when_the_call_runs_then_it_is_resolved_at_the_call() {
    let client = mock(vec![text_step("answer")]);
    let call = LlmCall {
        request: LlmRequest::Text(Source::Fn(Arc::new(|state, config| {
            Ok(format!(
                "{} Summarize: {}",
                config.str(&key("style"))?,
                state.str(&key("doc"))?
            ))
        }))),
        ..summarize(&client, Output::Set(key("summary")))
    };

    finished(run_in(&single(call), state_with("tides", &[]), &ctx()).await);

    assert_eq!(
        requests(&client)[0].messages(),
        [user_text("Be terse. Summarize: tides")]
    );
}

#[tokio::test]
async fn given_a_failing_client_when_the_call_runs_then_model_and_nothing_written() {
    let client = failing(ClientError::RateLimited { retry_after: None });
    let graph = single(summarize(&client, Output::Set(key("summary"))));

    let failure = run_in(&graph, state_with("doc", &[]), &ctx())
        .await
        .err()
        .unwrap();

    assert!(matches!(
        returned(&failure.error),
        GraphError::Model(ClientError::RateLimited { retry_after: None })
    ));
    assert_eq!(failure.checkpoint.state.str(&key("summary")).unwrap(), "");
}

#[tokio::test]
async fn given_a_streamed_answer_when_the_call_runs_then_the_observer_gets_each_event_with_its_key()
{
    for (request, expected_key) in [
        (LlmRequest::Text(Source::State(key("doc"))), "summary"),
        (
            LlmRequest::Conversation {
                key: key("chat"),
                author: author(),
            },
            "chat",
        ),
    ] {
        let client = mock(vec![text_step("answer")]);
        let call = LlmCall {
            request,
            ..summarize(&client, Output::Set(key("summary")))
        };
        let recorder = Arc::new(StreamRecorder::default());
        let observer: Arc<dyn Observer> = recorder.clone();
        let ctx = Context::new(observer, Arc::new(SeqIds::new()));

        finished(run_in(&single(call), state_with("doc", &[]), &ctx).await);

        let events = recorder.events();
        let derived = derive_events(&text_step("answer"), &Chunking::none()).unwrap();
        assert_eq!(
            events.len(),
            derived.events().len(),
            "every event of the reply reaches the observer, the finish included"
        );
        assert!(
            events
                .iter()
                .all(|(occurrence, key, _)| occurrence == "call" && key == expected_key),
            "{events:?}"
        );
    }
}
