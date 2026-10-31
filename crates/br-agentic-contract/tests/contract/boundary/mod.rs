use std::sync::atomic::Ordering;

use br_agentic_contract::memory::Call;
use br_agentic_contract::{
    InboxItem, InboxPort, Queue, Session, SessionError, ThreadAccess, store_at_boundary,
};
use br_llm_messages::{
    ClosureReason, EntryId, IncomingEntry, InputTurn, Turn, TurnClosure, TurnEntry,
};

use crate::common::*;
use crate::flaky::Flaky;

async fn run(
    session: &mut Session<'_>,
    world: &World,
    queue: &mut Queue,
) -> Result<br_agentic_contract::Boundary, br_agentic_contract::BoundaryError> {
    store_at_boundary(
        session,
        &world.inbox,
        &world.context(),
        queue,
        &mail_entry,
        turn_id(),
    )
    .await
}

fn queue_of(items: &[InboxItem]) -> Queue {
    let mut queue = Queue::new();
    for item in items {
        queue.push(item.clone());
    }
    queue
}

fn human_item(text: &str, world: &World) -> InboxItem {
    let entry = human(text);
    world.inbox.deliver_entry(world.agent, entry.clone());
    InboxItem::Entry(entry)
}

async fn start_with_trigger(world: &World) {
    world
        .access
        .add_input_turn(
            &world.thread,
            InputTurn::new(turn_id(), vec![human("trigger").into_entry()]).unwrap(),
        )
        .await
        .unwrap();
}

fn marks(world: &World) -> Vec<EntryId> {
    world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::InboxMarkRead { item, .. } => Some(*item),
            _ => None,
        })
        .collect()
}

fn position(world: &World, wanted: impl Fn(&Call) -> bool) -> usize {
    world.record.calls().iter().position(wanted).unwrap()
}

#[test]
fn given_an_item_whose_id_is_held_earlier_in_the_queue_when_it_is_pushed_then_it_is_skipped() {
    let world = World::new();
    let item = human_item("once", &world);
    let mut queue = Queue::new();

    assert!(queue.push(item.clone()));
    assert!(!queue.push(item));
    assert_eq!(queue.len(), 1);
}

#[tokio::test]
async fn given_the_inbox_when_the_queue_reads_it_then_it_holds_every_unread_item_of_all_batches_in_order()
 {
    let world = World::new();
    let items: Vec<InboxItem> = (0..5)
        .map(|n| human_item(&format!("m{n}"), &world))
        .collect();
    let mut queue = Queue::new();

    let added = queue
        .read(&world.inbox, &world.context(), limit(2))
        .await
        .unwrap();

    assert_eq!(added, 5);
    assert_eq!(queue.items(), items.as_slice());
    let again = queue
        .read(&world.inbox, &world.context(), limit(2))
        .await
        .unwrap();
    assert_eq!(again, 0);
}

#[tokio::test]
async fn given_no_open_ai_turn_when_the_boundary_stores_items_then_they_form_one_new_input_turn_with_one_entry_for_each_item_in_arrival_order()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let items = [
        human_item("first", &world),
        human_item("second", &world),
        human_item("third", &world),
    ];
    let mut session = world.session().await;
    let turns_before = session.conversation().turns().len();
    let mut queue = queue_of(&items);

    let report = run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    assert_eq!(conversation.turns().len(), turns_before + 1);
    let Some(Turn::InputTurn(input)) = conversation.turns().last() else {
        panic!("a new input turn")
    };
    let ids: Vec<EntryId> = input.entries().iter().map(IncomingEntry::id).collect();
    assert_eq!(ids, items.iter().map(InboxItem::id).collect::<Vec<_>>());
    assert_eq!(report.stored, ids);
    assert!(queue.is_empty());
}

#[tokio::test]
async fn given_a_thread_ending_with_an_input_turn_when_the_boundary_stores_an_item_then_that_input_turn_is_not_extended()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let before = world.access.conversation();
    let item = human_item("late", &world);
    let mut session = world.session().await;
    let mut queue = queue_of(&[item]);

    run(&mut session, &world, &mut queue).await.unwrap();

    let after = world.access.conversation();
    assert_eq!(after.turns()[0], before.turns()[0]);
    assert_eq!(after.turns().len(), 2);
}

#[tokio::test]
async fn given_an_open_ai_turn_of_the_agent_when_the_boundary_stores_items_then_each_is_added_to_it_after_its_last_entry_in_arrival_order()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (turn, _call) = world.open_turn_with_call(&mut session).await;
    let items = [human_item("one", &world), human_item("two", &world)];
    let mut queue = queue_of(&items);

    run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    let ai = conversation.turn(&turn.id).unwrap().as_ai().unwrap();
    let tail: Vec<EntryId> = ai
        .entries()
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(TurnEntry::id)
        .collect();
    assert_eq!(tail, items.iter().map(InboxItem::id).collect::<Vec<_>>());
    assert_eq!(conversation.turns().len(), 1);
}

#[tokio::test]
async fn given_a_closed_ai_turn_of_the_agent_when_the_boundary_stores_an_item_then_a_new_input_turn_holds_it()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    session
        .add_entry(&turn, TurnEntry::from(response_text(entry_id(), "done")))
        .await
        .unwrap();
    session
        .close_turn(
            &turn,
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await
        .unwrap();
    let item = human_item("after", &world);
    let mut queue = queue_of(&[item]);

    run(&mut session, &world, &mut queue).await.unwrap();

    assert!(matches!(
        world.access.conversation().turns().last(),
        Some(Turn::InputTurn(_))
    ));
}

#[tokio::test]
async fn given_an_open_ai_turn_of_another_agent_when_the_boundary_stores_an_item_then_a_new_input_turn_holds_it_and_the_other_turn_is_unchanged()
 {
    let world = World::new();
    let mut session = world.session().await;
    let mut other = world.ai(turn_id());
    other.agent = agent_id();
    other.author = author("critic");
    session
        .add_entry(&other, TurnEntry::from(response_text(entry_id(), "theirs")))
        .await
        .unwrap();
    let before = world.access.conversation().turns()[0].clone();
    let item = human_item("mine", &world);
    let mut queue = queue_of(&[item]);

    run(&mut session, &world, &mut queue).await.unwrap();

    let after = world.access.conversation();
    assert_eq!(after.turns()[0], before);
    assert!(matches!(after.turns().last(), Some(Turn::InputTurn(_))));
}

#[tokio::test]
async fn given_an_item_whose_entry_the_conversation_holds_when_the_boundary_runs_then_it_is_skipped_and_marked_as_read_and_not_stored_again()
 {
    let world = World::new();
    let item = human_item("held", &world);
    let InboxItem::Entry(entry) = &item else {
        panic!()
    };
    world
        .access
        .add_input_turn(
            &world.thread,
            InputTurn::new(turn_id(), vec![entry.entry().clone()]).unwrap(),
        )
        .await
        .unwrap();
    let mut session = world.session().await;
    let turns = session.conversation().turns().len();
    let mut queue = queue_of(std::slice::from_ref(&item));

    let report = run(&mut session, &world, &mut queue).await.unwrap();

    assert_eq!(report.skipped, [item.id()]);
    assert!(report.stored.is_empty());
    assert_eq!(world.access.conversation().turns().len(), turns);
    assert!(world.inbox.is_read(world.agent, item.id()));
    assert!(queue.is_empty());
}

mod builder;
mod entries;
mod inbox_errors;
