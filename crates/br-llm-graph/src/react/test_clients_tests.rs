//! The lax client serves its steps as they are, but refuses a list of
//! messages no client accepts, as every client library does.

use br_llm_clients_core::{BoxFuture, Client, ClientError, ClientRequest, OutputMode, StreamSink};
use br_llm_messages::{AssistantBlock, StreamEvent, Text, WireMessage, WireUserBlock};

use crate::react::test_clients::LaxClient;
use crate::react::test_support::text_step;

/// A sink that drops every stream event.
struct Discard;

impl StreamSink for Discard {
    fn send<'a>(&'a mut self, _event: StreamEvent) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

fn request(messages: Vec<WireMessage>) -> ClientRequest {
    ClientRequest::new(None, messages, Vec::new(), OutputMode::Plain).unwrap()
}

#[tokio::test]
async fn given_a_list_of_messages_starting_with_the_assistant_when_called_then_the_lax_client_refuses_it_and_keeps_its_step()
 {
    let client = LaxClient::new(vec![text_step("served")]);
    // Built, as its shape is one a request takes, but no client accepts it.
    let invalid = request(vec![WireMessage::Assistant {
        content: vec![AssistantBlock::Text {
            text: Text::new("I speak first.").unwrap(),
        }],
    }]);
    let valid = request(vec![WireMessage::User {
        content: vec![WireUserBlock::Text {
            text: Text::new("hi").unwrap(),
        }],
    }]);

    let refused = client.call(&invalid, &mut Discard).await;
    let served = client.call(&valid, &mut Discard).await;

    assert!(
        matches!(refused.result(), Err(ClientError::InvalidMessages { .. })),
        "{:?}",
        refused.result()
    );
    assert_eq!(served.result().unwrap(), &text_step("served"));
    assert_eq!(client.requests(), [invalid, valid]);
}
