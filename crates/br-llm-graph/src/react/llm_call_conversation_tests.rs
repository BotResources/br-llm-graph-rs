//! `LlmCall` on a conversation request: the conversation rendered for the
//! call's author.

use crate::graph::Output;
use crate::react::helpers::wire;
use crate::react::llm_call::{LlmCall, LlmRequest};
use crate::react::llm_call_support::*;
use crate::react::test_support::{author, key, mock, requests, text_step};

#[tokio::test]
async fn given_a_conversation_request_when_the_call_runs_then_it_is_rendered_for_the_author() {
    let client = mock(vec![text_step("The moon.")]);
    let call = LlmCall {
        request: LlmRequest::Conversation {
            key: key("chat"),
            author: author(),
        },
        ..summarize(&client, Output::Set(key("summary")))
    };
    let state = state_with("doc", &[]);
    let rendered = wire(state.conversation(&key("chat")).unwrap(), &author()).unwrap();

    let state = finished(run_in(&single(call), state, &ctx()).await);

    assert_eq!(requests(&client)[0].messages(), rendered.as_slice());
    assert_eq!(
        rendered.len(),
        1,
        "the librarian's turn is framed into the user message for another author"
    );
    assert_eq!(state.conversation(&key("chat")).unwrap().entries().len(), 2);
    assert_eq!(state.str(&key("summary")).unwrap(), "The moon.");
}
