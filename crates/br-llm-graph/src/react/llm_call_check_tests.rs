//! The build checks of `LlmCall`: its source keys, its conversation key and
//! its output key, each refused with the error its use would give.

use crate::error::GraphError;
use crate::graph::{CheckSite, Node, Output};
use crate::react::llm_call::{LlmCall, LlmRequest};
use crate::react::llm_call_support::*;
use crate::react::source::Source;
use crate::react::test_support::{author, key, mock};
use crate::state::{Kind, Value};

#[test]
fn given_source_keys_that_hold_no_text_when_built_then_source_key_mismatch() {
    let client = mock(Vec::new());
    let base = || summarize(&client, Output::Set(key("summary")));
    for (call, name) in [
        (
            LlmCall {
                system: Source::State(key("count")),
                ..base()
            },
            "count",
        ),
        (
            LlmCall {
                system: Source::Config(key("missing")),
                ..base()
            },
            "missing",
        ),
        (
            LlmCall {
                request: LlmRequest::Text(Source::State(key("chat"))),
                ..base()
            },
            "chat",
        ),
        (
            LlmCall {
                request: LlmRequest::Text(Source::Config(key("doc"))),
                ..base()
            },
            "doc",
        ),
    ] {
        assert!(
            matches!(
                call.check(&schema(), CheckSite::Graph),
                Err(GraphError::SourceKeyMismatch { key: found }) if found == key(name)
            ),
            "{name}"
        );
    }
    let list_request = LlmCall {
        request: LlmRequest::Text(Source::State(key("docs"))),
        ..base()
    };
    assert!(list_request.check(&schema(), CheckSite::Graph).is_ok());
}

#[test]
fn given_a_conversation_key_that_is_not_a_conversation_when_built_then_the_key_or_kind_error() {
    let client = mock(Vec::new());
    let on = |name: &str| LlmCall {
        request: LlmRequest::Conversation {
            key: key(name),
            author: author(),
        },
        ..summarize(&client, Output::Set(key("summary")))
    };
    assert!(matches!(
        on("missing").check(&schema(), CheckSite::Graph),
        Err(GraphError::UnknownKey { key: found }) if found == key("missing")
    ));
    assert!(matches!(
        on("doc").check(&schema(), CheckSite::Graph),
        Err(GraphError::KindMismatch { key: named, expected: Kind::Conversation, found: "str" })
            if named == key("doc")
    ));
    assert!(on("chat").check(&schema(), CheckSite::Graph).is_ok());
}

#[test]
fn given_an_output_that_cannot_take_a_string_when_built_then_the_error_writing_it_would_give() {
    let client = mock(Vec::new());
    let checked = |output: Output| summarize(&client, output).check(&schema(), CheckSite::Graph);
    assert!(matches!(
        checked(Output::Set(key("missing"))),
        Err(GraphError::UnknownKey { key: found }) if found == key("missing")
    ));
    assert!(matches!(
        checked(Output::Set(key("count"))),
        Err(GraphError::KindMismatch { key: named, expected: Kind::Int, found: "str" })
            if named == key("count")
    ));
    assert!(matches!(
        checked(Output::Append(key("missing"))),
        Err(GraphError::UnknownKey { key: found }) if found == key("missing")
    ));
    assert!(matches!(
        checked(Output::Append(key("summary"))),
        Err(GraphError::AppendNotList { key: found }) if found == key("summary")
    ));
    assert!(matches!(
        checked(Output::Append(key("counts"))),
        Err(GraphError::KindMismatch { key: named, expected: Kind::Int, found: "str" })
            if named == key("counts")
    ));
    assert!(checked(Output::Set(key("summary"))).is_ok());
    assert!(checked(Output::Append(key("summaries"))).is_ok());
}

#[test]
fn given_an_output_check_when_compared_with_applying_the_answer_then_the_same_error() {
    // The build refusals above are the ones the run would meet writing a
    // string through the output.
    let state = state_with("doc", &[]);
    assert!(matches!(
        state.clone().apply_batch(&[crate::update::Update::Set {
            key: key("count"),
            value: Value::str("x"),
        }]),
        Err(GraphError::KindMismatch {
            expected: Kind::Int,
            found: "str",
            ..
        })
    ));
    assert!(matches!(
        state.clone().apply_batch(&[crate::update::Update::Append {
            key: key("counts"),
            value: Value::str("x"),
        }]),
        Err(GraphError::KindMismatch {
            expected: Kind::Int,
            found: "str",
            ..
        })
    ));
}
