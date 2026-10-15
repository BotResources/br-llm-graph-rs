//! `LlmCall` on a conversation request: the conversation rendered for the
//! call's author, and a conversation that renders to nothing.

use br_llm_clients_mock::MockClient;
use br_llm_messages::{
    AssistantBlock, Conversation, MessageError, Text, Turn, TurnId, WireMessage,
};

use crate::error::GraphError;
use crate::react::helpers::wire;
use crate::react::llm_call::{LlmCall, LlmRequest};
use crate::react::llm_call_support::*;
use crate::react::test_support::{author, key, mock, requests, returned, skey, text_step};
use crate::state::{State, Value};
use crate::update::Update;

/// A call answering on the conversation `chat`, rendered for `author()`.
fn on_chat(client: &MockClient) -> LlmCall {
    LlmCall {
        request: LlmRequest::Conversation {
            key: skey("chat"),
            author: author(),
        },
        ..summarize(client, set_summary())
    }
}

/// The seeded chat (a question, a turn of the librarian) followed by a turn
/// of the call's author.
fn with_an_own_turn() -> State {
    let mut state = state_with("doc", &[]);
    state
        .apply_batch(&[Update::PushTurn {
            key: key("chat"),
            turn: Turn::new(
                TurnId::new("t1").unwrap(),
                Some(author()),
                text_step("Mostly the moon, and the sun a little."),
            ),
        }])
        .unwrap();
    state
}

#[tokio::test]
async fn given_a_conversation_request_when_the_call_runs_then_it_is_rendered_for_the_author() {
    let client = mock(vec![text_step("The moon.")]);
    let state = with_an_own_turn();
    let rendered = wire(state.conversation(&key("chat")).unwrap(), &author()).unwrap();

    let state = finished(run_in(&single(on_chat(&client)), state, &ctx()).await);

    let sent = requests(&client)[0].messages().to_vec();
    assert_eq!(sent, rendered);
    let own = vec![AssistantBlock::Text {
        text: Text::new("Mostly the moon, and the sun a little.").unwrap(),
    }];
    assert!(
        matches!(
            sent.as_slice(),
            [WireMessage::User { content }, WireMessage::Assistant { content: mine }]
                if content.len() == 2 && *mine == own
        ),
        "the author's own turn is an assistant message, the question and the librarian's \
         turn framed user text: {sent:?}"
    );
    assert_eq!(
        state.conversation(&key("chat")).unwrap().entries().len(),
        3,
        "the call keeps no conversation"
    );
    assert_eq!(state.str(&key("summary")).unwrap(), "The moon.");
}

#[tokio::test]
async fn given_an_empty_conversation_when_the_call_runs_then_the_node_fails_before_any_call() {
    let client = mock(vec![text_step("unused")]);
    let mut state = state_with("doc", &[]);
    state
        .apply_batch(&[Update::Set {
            key: key("chat"),
            value: Value::conversation(Conversation::new()),
        }])
        .unwrap();

    let failure = run_in(&single(on_chat(&client)), state, &ctx())
        .await
        .err()
        .unwrap();

    assert!(
        matches!(
            returned(&failure.error),
            GraphError::Message(MessageError::EmptyRender)
        ),
        "the render refuses a conversation with nothing to send: {}",
        failure.error
    );
    assert_eq!(client.calls(), 0, "no request reaches the client");
    assert_eq!(failure.checkpoint.state.str(&key("summary")).unwrap(), "");
}
