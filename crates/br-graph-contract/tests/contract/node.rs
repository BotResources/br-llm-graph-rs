use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use br_graph_contract::{
    CancelToken, CancelView, CreateError, CreateFuture, Event, EventEndError, GraphBuilder, Input,
    Instance, InstanceId, Node, PROGRAMMATIC,
};
use tokio::time::timeout;

use crate::common::{Tally, TallyUpdate, custom_node, edge, id, plain, state};
use crate::park::{counting_waker, poll_with};

fn input(total: i64) -> Input<Tally> {
    Input::new(Arc::new(state(total)))
}

#[test]
fn given_nodes_of_one_subkind_when_a_graph_is_built_then_the_structure_names_each_kind_and_subkind()
{
    let graph = GraphBuilder::new("1")
        .node(plain("review"))
        .node(custom_node("check", "agent", "worker"))
        .entry("review")
        .entry("check")
        .edge(edge("review", &["END"]))
        .edge(edge("check", &["END"]))
        .build()
        .unwrap();
    let structure = graph.structure();
    let described: Vec<(&str, &str, &str)> = structure
        .nodes()
        .iter()
        .map(|node| {
            (
                node.name.as_str(),
                node.kind.as_str(),
                node.subkind.as_str(),
            )
        })
        .collect();
    assert_eq!(
        described,
        vec![
            ("check", "agent", "worker"),
            ("review", PROGRAMMATIC, "worker")
        ]
    );
    let held = graph.node("review").unwrap();
    assert_eq!(held.name(), "review");
    assert!(graph.node("ghost").is_none());
}

#[tokio::test]
async fn given_a_node_when_it_creates_an_instance_then_a_given_input_ends_it_with_one_result() {
    let node = plain("review");
    let token = CancelToken::new();
    let instance = node.create(token.view(), id(1)).await.unwrap();
    assert_eq!(instance.id(), id(1));
    let (input_end, mut events) = instance.into_parts();
    input_end.give(input(7)).unwrap();
    assert!(matches!(
        events.recv().await,
        Ok(Event::Result(TallyUpdate::Add(7)))
    ));
    assert!(matches!(events.recv().await, Err(EventEndError::Finished)));
}

#[tokio::test]
async fn given_one_node_when_two_instances_run_then_they_never_see_each_others_inputs() {
    let node = plain("review");
    let token = CancelToken::new();
    let first = node.create(token.view(), id(1)).await.unwrap();
    let second = node.create(token.view(), id(2)).await.unwrap();
    let (first_input, mut first_events) = first.into_parts();
    let (second_input, mut second_events) = second.into_parts();
    first_input.give(input(1)).unwrap();
    second_input.give(input(2)).unwrap();
    assert!(matches!(
        second_events.recv().await,
        Ok(Event::Result(TallyUpdate::Add(2)))
    ));
    assert!(matches!(
        first_events.recv().await,
        Ok(Event::Result(TallyUpdate::Add(1)))
    ));
}

#[tokio::test]
async fn given_the_same_node_when_a_new_instance_is_made_after_an_end_then_it_starts_from_nothing()
{
    let node = plain("review");
    let token = CancelToken::new();
    let first = node.create(token.view(), id(1)).await.unwrap();
    let (input_end, mut events) = first.into_parts();
    input_end.give(input(1)).unwrap();
    assert!(events.recv().await.is_ok());
    let second = node.create(token.view(), id(2)).await.unwrap();
    assert_ne!(second.id(), id(1));
    let (input_end, mut events) = second.into_parts();
    input_end.give(input(1)).unwrap();
    assert!(matches!(
        events.recv().await,
        Ok(Event::Result(TallyUpdate::Add(1)))
    ));
}

struct Refusing;

impl Node<Tally> for Refusing {
    fn name(&self) -> &str {
        "refusing"
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "gate"
    }

    fn create(&self, _cancel: CancelView, _id: InstanceId) -> CreateFuture<'_, Tally> {
        Box::pin(async { Err(CreateError::new("no capacity")) })
    }
}

#[tokio::test]
async fn given_a_node_that_cannot_create_when_asked_then_the_error_keeps_the_opaque_cause() {
    let token = CancelToken::new();
    let error = Refusing.create(token.view(), id(1)).await.unwrap_err();
    assert_eq!(error.cause().to_string(), "no capacity");
    assert_eq!(error.into_cause().to_string(), "no capacity");
}

struct Waiting;

impl Node<Tally> for Waiting {
    fn name(&self) -> &str {
        "waiting"
    }

    fn kind(&self) -> &str {
        PROGRAMMATIC
    }

    fn subkind(&self) -> &str {
        "waiter"
    }

    fn create(&self, cancel: CancelView, id: InstanceId) -> CreateFuture<'_, Tally> {
        Box::pin(async move {
            let (instance, _reader, sender) = Instance::<Tally>::channel(id);
            tokio::spawn(async move {
                cancel.triggered().await;
                sender.finish(Event::Cancelled);
            });
            Ok(instance)
        })
    }
}

#[tokio::test]
async fn given_a_run_token_when_triggered_then_an_instance_holding_the_view_ends_cancelled() {
    let run = CancelToken::new();
    let instance = Waiting.create(run.view(), id(1)).await.unwrap();
    let (_input_end, mut events) = instance.into_parts();
    {
        let (_wakes, waker) = counting_waker();
        let mut recv = pin!(events.recv());
        assert!(poll_with(recv.as_mut(), &waker).is_pending());
    }
    run.handle().trigger();
    assert!(matches!(
        timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap(),
        Ok(Event::Cancelled)
    ));
}

#[tokio::test]
async fn given_a_child_token_of_a_node_when_triggered_then_the_run_is_not_cancelled() {
    let run = CancelToken::new();
    let own = run.view().child();
    own.handle().trigger();
    assert!(own.view().is_triggered());
    assert!(!run.view().is_triggered());
}
