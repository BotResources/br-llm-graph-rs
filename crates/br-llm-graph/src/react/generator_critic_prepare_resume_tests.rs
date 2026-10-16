//! A durable generator-critic whose `prepare` writes the trigger into
//! `opening`, interrupted once the input is stored: called again on its
//! thread, `prepare` writes the same input, which the thread already holds,
//! so the thread keeps one input and the call goes on once.

use std::sync::Arc;

use br_llm_messages::Conversation;

use crate::graph::{Context, Node, NodeFuture};
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::*;
use crate::react::generator_critic_support::*;
use crate::react::react_durable_support::{Host, nth_thread, said};
use crate::react::test_support::*;
use crate::state::{Config, State};
use crate::testkit::{Op, TestStore};
use crate::update::Update;

/// Writes the user input `q1` into `opening`, as a host's node reading the
/// question from a service would.
struct AskQuestion;

impl Node for AskQuestion {
    fn run<'a>(
        &'a self,
        _state: &'a State,
        _config: &'a Config,
        _ctx: &'a Context,
    ) -> NodeFuture<'a> {
        Box::pin(async {
            Ok(vec![Update::Input {
                key: key("opening"),
                input: said("q1", "explain the tides"),
            }])
        })
    }
}

#[tokio::test]
async fn given_a_durable_call_with_prepare_interrupted_after_its_input_is_stored_when_called_again_then_the_thread_holds_one_input_and_one_answer()
 {
    let store = Arc::new(TestStore::new());
    store.fail(Op::OpenTurn, 1);
    let generator_client = mock(vec![text_step("draft")]);
    let critic_client = mock(vec![verdict(true, "Fine")]);
    let graph = GeneratorCritic {
        prepare: Some(Box::new(AskQuestion)),
        ..durable_duo(&generator_client, &critic_client, &store)
    }
    .graph()
    .unwrap();
    let host = Host::new();

    let first = call_on(&host, &graph, Conversation::new(), "", "r1").await;
    assert!(first.result.is_err(), "the generator's turn could not open");
    let thread = nth_thread(1);
    assert_eq!(shape(&store.view(&thread).unwrap().conversation), ["user"]);

    let done = ended(
        call_on(&host, &graph, Conversation::new(), thread.as_str(), "r1").await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "draft");
    assert_eq!(
        shape(&store.view(&thread).unwrap().conversation),
        ["user", "writer:draft", "reviewer:Fine"]
    );
    let stored = writes(&store)
        .into_iter()
        .filter(|line| line.starts_with("input "))
        .count();
    assert_eq!(stored, 1, "the input is stored once: {:?}", writes(&store));
    assert_eq!((generator_client.calls(), critic_client.calls()), (1, 1));
}
