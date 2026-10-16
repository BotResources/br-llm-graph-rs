//! A message of the critic cut while it was written to the thread: once
//! failed and repaired by the store, it is no judgment of the critic, so a
//! resumed call has the critic judge the same answer again, exactly as an
//! uninterrupted call would; it is not counted against the budget.

use std::sync::Arc;

use br_llm_clients_mock::MockClient;

use crate::graph::Count;
use crate::react::generator_critic::GeneratorCritic;
use crate::react::generator_critic_durable_support::*;
use crate::react::generator_critic_support::*;
use crate::react::react_durable_support::{Host, nth_thread, opening, said};
use crate::react::test_support::*;
use crate::testkit::{Op, TestStore};

use crate::react::generator_critic_support::{critic as critic_seat, generator as generator_seat};

/// The durable generator-critic on the two clients in `store`, with a
/// budget of one critique and no final generation.
fn one_critique(
    generator: &MockClient,
    critic: &MockClient,
    store: &Arc<TestStore>,
) -> GeneratorCritic {
    GeneratorCritic {
        thread_store: Some(store.clone()),
        final_generation: false,
        ..GeneratorCritic::new(
            generator_seat(generator),
            critic_seat(critic),
            Count::Fixed(1),
        )
    }
}

/// The stream events an uninterrupted call of `graph` pushes into its
/// thread: its last is the last event of the last message of the critic.
async fn pushes_of(graph: impl Fn(&Arc<TestStore>) -> GeneratorCritic) -> usize {
    let store = Arc::new(TestStore::new());
    let graph = graph(&store).graph().unwrap();
    let host = Host::new();
    call_on(&host, &graph, opening(&[said("i1", "explain")]), "", "r1")
        .await
        .result
        .map_err(|failure| failure.error)
        .unwrap();
    store.calls(Op::PushEvent)
}

#[tokio::test]
async fn given_an_acceptance_cut_while_written_when_the_call_resumes_then_the_critic_judges_the_same_answer_again()
 {
    let pushes = pushes_of(|store| {
        durable_duo(
            &mock(vec![text_step("draft")]),
            &mock(vec![verdict(true, "Fine")]),
            store,
        )
    })
    .await;
    let store = Arc::new(TestStore::new());
    store.fail(Op::PushEvent, pushes);
    let generator_client = mock(vec![text_step("draft"), text_step("second")]);
    let critic_client = mock(vec![verdict(true, "Fine"), verdict(true, "Fine")]);
    let graph = durable_duo(&generator_client, &critic_client, &store)
        .graph()
        .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "explain")]);
    assert!(
        call_on(&host, &graph, question.clone(), "", "r1")
            .await
            .result
            .is_err()
    );

    let thread = nth_thread(1);
    let done = ended(
        call_on(&host, &graph, question, thread.as_str(), "r1").await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "draft");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(
        (generator_client.calls(), critic_client.calls()),
        (1, 2),
        "the critic judges again, the generator does not answer again: {:?}",
        shape(&store.view(&thread).unwrap().conversation)
    );
}

#[tokio::test]
async fn given_a_critique_cut_while_written_when_the_call_resumes_then_the_critic_judges_the_same_answer_again_uncounted()
 {
    let pushes = pushes_of(|store| {
        one_critique(
            &mock(vec![text_step("draft")]),
            &mock(vec![verdict(false, "fix it")]),
            store,
        )
    })
    .await;
    let store = Arc::new(TestStore::new());
    store.fail(Op::PushEvent, pushes);
    let generator_client = mock(vec![text_step("draft"), text_step("second")]);
    let critic_client = mock(vec![verdict(false, "fix it"), verdict(true, "Fine")]);
    let graph = one_critique(&generator_client, &critic_client, &store)
        .graph()
        .unwrap();
    let host = Host::new();
    let question = opening(&[said("i1", "explain")]);
    assert!(
        call_on(&host, &graph, question.clone(), "", "r1")
            .await
            .result
            .is_err()
    );

    let thread = nth_thread(1);
    let done = ended(
        call_on(&host, &graph, question, thread.as_str(), "r1").await,
        "validated",
    );

    assert_eq!(done.str(&key("answer")).unwrap(), "draft");
    assert_eq!(done.int(&key("nb_generations")).unwrap(), 1);
    assert_eq!(
        (generator_client.calls(), critic_client.calls()),
        (1, 2),
        "the cut critique is not counted against the budget of one: {:?}",
        shape(&store.view(&thread).unwrap().conversation)
    );
}
