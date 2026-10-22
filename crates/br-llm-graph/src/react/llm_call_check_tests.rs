//! The build checks of `LlmCall`: the keys its functions declare, its
//! conversation key and its output key, each refused with the error its use
//! would give.

use crate::error::GraphError;
use crate::graph::{CheckSite, Node, TextOutput};
use crate::react::given::Prompt;
use crate::react::llm_call::{LlmCall, LlmRequest};
use crate::react::llm_call_support::*;
use crate::react::test_support::{author, ckey, key, mock, skey};
use crate::state::{Kind, Value};

#[test]
fn given_function_keys_the_graph_lacks_or_holds_with_another_kind_when_built_then_refused() {
    let client = mock(Vec::new());
    let base = || summarize(&client, set_summary());
    let unknown = |call: LlmCall, name: &str| {
        assert!(
            matches!(
                call.check(&schema(), CheckSite::Graph),
                Err(GraphError::UnknownKey { key: found }) if found == key(name)
            ),
            "{name}"
        );
    };
    unknown(
        LlmCall {
            system: Prompt::from_config(&ckey("missing")),
            ..base()
        },
        "missing",
    );
    unknown(
        LlmCall {
            request: LlmRequest::Text(Prompt::from_config(&ckey("doc"))),
            ..base()
        },
        "doc",
    );
    let mismatch = |call: LlmCall, name: &str, expected: Kind, found: &str| {
        assert!(
            matches!(
                call.check(&schema(), CheckSite::Graph),
                Err(GraphError::KindMismatch { key: named, expected: wanted, found: seen })
                    if named == key(name) && wanted == expected && seen == found
            ),
            "{name}"
        );
    };
    mismatch(
        LlmCall {
            system: Prompt::from_state(&skey("count")),
            ..base()
        },
        "count",
        Kind::Str,
        "int",
    );
    mismatch(
        LlmCall {
            request: LlmRequest::Text(Prompt::from_state(&skey("chat"))),
            ..base()
        },
        "chat",
        Kind::Str,
        "conversation",
    );
    mismatch(
        LlmCall {
            request: LlmRequest::Text(Prompt::from_state(&skey("docs"))),
            ..base()
        },
        "docs",
        Kind::Str,
        "list",
    );
    let list_request = LlmCall {
        request: LlmRequest::Text(Prompt::from_state_list(&skey("docs"))),
        ..base()
    };
    assert!(list_request.check(&schema(), CheckSite::Graph).is_ok());
}

#[test]
fn given_a_conversation_key_that_is_not_a_conversation_when_built_then_the_key_or_kind_error() {
    let client = mock(Vec::new());
    let on = |name: &str| LlmCall {
        request: LlmRequest::Conversation {
            key: skey(name),
            author: author(),
        },
        ..summarize(&client, set_summary())
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
    let checked =
        |output: TextOutput| summarize(&client, output).check(&schema(), CheckSite::Graph);
    assert!(matches!(
        checked(TextOutput::Set(skey("missing"))),
        Err(GraphError::UnknownKey { key: found }) if found == key("missing")
    ));
    assert!(matches!(
        checked(TextOutput::Set(skey("count"))),
        Err(GraphError::KindMismatch { key: named, expected: Kind::Int, found: "str" })
            if named == key("count")
    ));
    assert!(matches!(
        checked(TextOutput::Append(skey("missing"))),
        Err(GraphError::UnknownKey { key: found }) if found == key("missing")
    ));
    assert!(matches!(
        checked(TextOutput::Append(skey("summary"))),
        Err(GraphError::AppendNotList { key: found }) if found == key("summary")
    ));
    assert!(matches!(
        checked(TextOutput::Append(skey("counts"))),
        Err(GraphError::KindMismatch { key: named, expected: Kind::Int, found: "str" })
            if named == key("counts")
    ));
    assert!(checked(set_summary()).is_ok());
    assert!(checked(append_summaries()).is_ok());
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
