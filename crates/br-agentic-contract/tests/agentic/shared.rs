use br_agentic_contract::{
    InboxItem, InboxPort, Incoming, Opened, Queue, Stored, ThreadAccess, ThreadError, TurnTarget,
    store_at_boundary,
};
use br_llm_messages::{ClosureReason, Turn, TurnClosure};

use crate::common::*;
use crate::flaky::Flaky;

fn limit() -> std::num::NonZeroUsize {
    std::num::NonZeroUsize::new(10).unwrap()
}

fn critic_target(turn: br_llm_messages::TurnId) -> TurnTarget {
    TurnTarget::Ai {
        id: turn,
        author: author("critic"),
    }
}

async fn critic_answers(world: &World) -> (br_llm_messages::AgentId, br_llm_messages::TurnId) {
    let critic = agent_id();
    world.hub.set_author(critic, author("critic"));
    let turn = turn_id();
    world
        .access
        .complete_response(
            &world.thread,
            critic,
            critic_target(turn),
            response_text(entry_id(), "critique"),
        )
        .await
        .unwrap();
    (critic, turn)
}

fn ai_entry_count(world: &World, turn: br_llm_messages::TurnId) -> usize {
    world
        .hub
        .conversation(&world.thread)
        .unwrap()
        .turn(&turn)
        .unwrap()
        .as_ai()
        .unwrap()
        .entries()
        .len()
}

#[tokio::test]
async fn given_a_thread_ending_with_the_ai_turn_of_another_agent_when_the_boundary_stores_an_item_then_a_new_input_turn_holds_it_and_the_other_turn_is_unchanged()
 {
    let world = World::new();
    world.start_with_human("start").await;
    let (_critic, foreign) = critic_answers(&world).await;
    let message = human("next");
    world.hub.deliver_entry(world.agent, message.clone());
    let mut queue = Queue::new();
    queue.push(InboxItem::Entry(message.clone()));
    let new_turn = turn_id();

    store_at_boundary(
        &world.access,
        &world.inbox,
        &world.context(),
        &mut queue,
        new_turn,
    )
    .await
    .unwrap();

    assert_eq!(ai_entry_count(&world, foreign), 1);
    let conversation = world.hub.conversation(&world.thread).unwrap();
    assert!(matches!(
        conversation.turn(&new_turn).unwrap(),
        Turn::InputTurn(_)
    ));
    assert!(world.projected().await.entry(&message.id()).is_ok());
}

#[tokio::test]
async fn given_the_ai_turn_of_another_agent_when_an_entry_is_added_to_it_as_an_input_turn_then_the_addition_is_refused()
 {
    let world = World::new();
    world.start_with_human("start").await;
    let (_critic, foreign) = critic_answers(&world).await;

    let error = world
        .access
        .add_incoming(
            &world.thread,
            world.agent,
            TurnTarget::Input(foreign),
            Incoming::Entry(human("intruder")),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ThreadError::Refused(_)));
    assert_eq!(ai_entry_count(&world, foreign), 1);
}

#[tokio::test]
async fn given_the_ai_turn_of_one_agent_when_another_agent_completes_opens_or_closes_on_it_then_each_is_refused_and_the_turn_is_unchanged()
 {
    let world = World::new();
    world.start_with_human("start").await;
    let (critic, foreign) = critic_answers(&world).await;

    let completed = world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(foreign),
            response_text(entry_id(), "intruder"),
        )
        .await;
    let opened = world
        .access
        .open_response(&world.thread, world.agent, world.ai(foreign), entry_id())
        .await;
    let closed = world
        .access
        .close_turn(
            &world.thread,
            world.agent,
            foreign,
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await;

    assert!(matches!(completed, Err(ThreadError::Refused(_))));
    assert!(matches!(opened, Err(ThreadError::Refused(_))));
    assert!(matches!(closed, Err(ThreadError::Refused(_))));
    assert_eq!(ai_entry_count(&world, foreign), 1);
    let own = world
        .access
        .open_response(&world.thread, critic, critic_target(foreign), entry_id())
        .await
        .unwrap();
    assert!(matches!(own, Opened::Stream(_)));
}

#[tokio::test]
async fn given_an_ai_turn_when_an_entry_is_added_with_another_author_name_then_the_addition_is_refused()
 {
    let world = World::new();
    world.start_with_human("start").await;
    let turn = turn_id();
    world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry_id(), "mine"),
        )
        .await
        .unwrap();

    let error = world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            TurnTarget::Ai {
                id: turn,
                author: author("someone else"),
            },
            response_text(entry_id(), "renamed"),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ThreadError::Refused(_)));
    assert_eq!(ai_entry_count(&world, turn), 1);
}

#[tokio::test]
async fn given_an_inbox_read_when_an_item_arrives_and_the_caller_then_waits_then_the_wait_returns_that_item()
 {
    let world = World::new();
    let first = human("first");
    world.hub.deliver_entry(world.agent, first.clone());
    let read = world
        .inbox
        .items(&world.context(), None, limit())
        .await
        .unwrap();
    assert_eq!(read.items.len(), 1);
    let second = human("second");
    world.hub.deliver_entry(world.agent, second.clone());

    let context = world.context();
    let waited = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        world.inbox.wait(&context),
    )
    .await
    .expect("the wait must not lose the item")
    .unwrap();

    assert_eq!(waited.id(), second.id());
}

#[tokio::test]
async fn given_an_input_turn_with_one_held_and_one_new_entry_when_it_is_added_then_only_the_new_entry_is_stored()
 {
    let world = World::new();
    let held = human("held");
    world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Entry(held.clone())],
        )
        .await
        .unwrap();
    let fresh = human("fresh");
    let turn = turn_id();

    let stored = world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn,
            vec![
                Incoming::Entry(held.clone()),
                Incoming::Entry(fresh.clone()),
            ],
        )
        .await
        .unwrap();

    assert_eq!(stored, Stored::Added);
    let conversation = world.hub.conversation(&world.thread).unwrap();
    let Turn::InputTurn(input) = conversation.turn(&turn).unwrap() else {
        panic!("expected an input turn");
    };
    assert_eq!(input.entries().len(), 1);
    assert!(world.projected().await.entry(&fresh.id()).is_ok());
}

#[tokio::test]
async fn given_a_held_item_then_a_failing_store_when_the_boundary_runs_then_the_error_carries_what_was_done_before_it()
 {
    let world = World::new();
    let held = human("held");
    world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Entry(held.clone())],
        )
        .await
        .unwrap();
    let fresh = human("fresh");
    let mut queue = Queue::new();
    queue.push(InboxItem::Entry(held.clone()));
    queue.push(InboxItem::Entry(fresh.clone()));
    let flaky = Flaky {
        inner: world.access.clone(),
        fail_adds: true,
        fail_discards: false,
    };

    let error = store_at_boundary(
        &flaky,
        &world.inbox,
        &world.context(),
        &mut queue,
        turn_id(),
    )
    .await
    .unwrap_err();

    assert_eq!(error.done.skipped, vec![held.id()]);
    assert!(error.done.stored.is_empty());
    let kept: Vec<_> = queue.items().iter().map(InboxItem::id).collect();
    assert_eq!(kept, vec![fresh.id()]);
}
