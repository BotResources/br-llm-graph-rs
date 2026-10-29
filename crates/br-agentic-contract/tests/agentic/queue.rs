use br_agentic_contract::{
    Boundary, InboxItem, InboxPort, Incoming, MailItem, Queue, ThreadAccess, store_at_boundary,
};
use br_llm_messages::{ClosureReason, EntryView, IncomingEntry, Turn, TurnClosure};

use crate::common::*;
use crate::flaky::Flaky;

async fn queue_all(world: &World, queue: &mut Queue) {
    let batch = world
        .inbox
        .items(
            &world.context(),
            None,
            std::num::NonZeroUsize::new(50).unwrap(),
        )
        .await
        .unwrap();
    for item in batch.items {
        queue.push(item);
    }
}

async fn send_from_known(world: &World, mail: br_agentic_contract::NewMail) {
    let sender = agent_id();
    world.hub.set_author(sender, author("sender"));
    world
        .inbox
        .send(&world.context_of(sender), mail)
        .await
        .unwrap();
}

#[tokio::test]
async fn given_a_queue_when_it_is_pushed_twice_with_one_identity_then_it_holds_the_item_once() {
    let mut queue = Queue::new();
    let entry = human("once");

    let first = queue.push(InboxItem::Entry(entry.clone()));
    let second = queue.push(InboxItem::Entry(entry));

    assert!(first && !second);
    assert_eq!(queue.len(), 1);
}

#[tokio::test]
async fn given_a_thread_ending_with_an_input_turn_when_the_boundary_stores_the_queue_then_the_entries_follow_in_arrival_order_and_mails_are_marked_read()
 {
    let world = World::new();
    world.start_with_human("start").await;
    let mail = new_mail(&[world.agent], &[], "m1");
    let later = human("later");
    send_from_known(&world, mail.clone()).await;
    world.hub.deliver_entry(world.agent, later.clone());
    let mut queue = Queue::new();
    queue_all(&world, &mut queue).await;

    let report = store_at_boundary(
        &world.access,
        &world.inbox,
        &world.context(),
        &mut queue,
        turn_id(),
    )
    .await
    .unwrap();

    assert_eq!(report.stored, vec![mail.id.entry_id(), later.id()]);
    let ids = entry_ids(&world.projected().await);
    assert_eq!(&ids[ids.len() - 2..], &[mail.id.entry_id(), later.id()]);
    assert_eq!(world.projected().await.turns().len(), 1);
    assert!(world.hub.is_read(world.agent, mail.id));
    assert!(queue.is_empty());
}

#[tokio::test]
async fn given_an_open_ai_turn_when_the_boundary_stores_the_queue_then_the_entries_are_added_to_it_after_its_last_entry()
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
            response_text(entry_id(), "ok"),
        )
        .await
        .unwrap();
    let notice = notice("note");
    let mut queue = Queue::new();
    queue.push(InboxItem::Entry(notice.clone()));

    store_at_boundary(
        &world.access,
        &world.inbox,
        &world.context(),
        &mut queue,
        turn_id(),
    )
    .await
    .unwrap();

    let projected = world.projected().await;
    let ai = projected.turns().last().and_then(Turn::as_ai).unwrap();
    assert_eq!(ai.entries().last().unwrap().id(), notice.id());
    assert_eq!(projected.turns().len(), 2);
}

#[tokio::test]
async fn given_a_thread_that_does_not_end_with_an_input_turn_when_the_boundary_stores_the_queue_then_one_new_input_turn_holds_one_entry_for_each_item()
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
            response_text(entry_id(), "ok"),
        )
        .await
        .unwrap();
    world
        .access
        .close_turn(
            &world.thread,
            world.agent,
            turn,
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await
        .unwrap();
    let one = human("one");
    let two = notice("two");
    let new_turn = turn_id();
    let mut queue = Queue::new();
    queue.push(InboxItem::Entry(one.clone()));
    queue.push(InboxItem::Entry(two.clone()));

    store_at_boundary(
        &world.access,
        &world.inbox,
        &world.context(),
        &mut queue,
        new_turn,
    )
    .await
    .unwrap();

    let projected = world.projected().await;
    let input = projected.turn(&new_turn).unwrap().as_input().unwrap();
    let ids: Vec<_> = input.entries().iter().map(IncomingEntry::id).collect();
    assert_eq!(ids, vec![one.id(), two.id()]);
    assert_eq!(projected.turns().last().unwrap().id(), new_turn);
}

#[tokio::test]
async fn given_an_empty_thread_when_the_boundary_stores_the_queue_then_a_new_input_turn_is_created()
{
    let world = World::new();
    let new_turn = turn_id();
    let entry = human("first");
    let mut queue = Queue::new();
    queue.push(InboxItem::Entry(entry.clone()));

    let report = store_at_boundary(
        &world.access,
        &world.inbox,
        &world.context(),
        &mut queue,
        new_turn,
    )
    .await
    .unwrap();

    assert_eq!(report.stored, vec![entry.id()]);
    assert!(world.projected().await.turn(&new_turn).is_ok());
}

#[tokio::test]
async fn given_an_item_the_projected_thread_holds_when_the_boundary_runs_then_it_is_skipped_and_its_mail_is_marked_read()
 {
    let world = World::new();
    let sender = agent_id();
    world.hub.set_author(sender, author("sender"));
    let mail = new_mail(&[world.agent], &[], "held");
    send_from_known(&world, mail.clone()).await;
    let batch = world
        .inbox
        .items(
            &world.context(),
            None,
            std::num::NonZeroUsize::new(5).unwrap(),
        )
        .await
        .unwrap();
    let item: MailItem = match &batch.items[0] {
        InboxItem::Mail(item) => item.clone(),
        InboxItem::Entry(_) => unreachable!(),
    };
    world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Mail(item)],
        )
        .await
        .unwrap();
    let before = world.hub.conversation(&world.thread).unwrap();
    let mut queue = Queue::new();
    queue.push(batch.items[0].clone());

    let report = store_at_boundary(
        &world.access,
        &world.inbox,
        &world.context(),
        &mut queue,
        turn_id(),
    )
    .await
    .unwrap();

    assert_eq!(
        report,
        Boundary {
            stored: Vec::new(),
            skipped: vec![mail.id.entry_id()]
        }
    );
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
    assert!(world.hub.is_read(world.agent, mail.id));
}

#[tokio::test]
async fn given_a_store_that_fails_when_the_boundary_runs_then_the_mail_is_not_marked_read_and_the_items_stay_queued_in_order()
 {
    let world = World::new();
    world.start_with_human("start").await;
    let first = new_mail(&[world.agent], &[], "first");
    let second = human("second");
    send_from_known(&world, first.clone()).await;
    world.hub.deliver_entry(world.agent, second.clone());
    let mut queue = Queue::new();
    queue_all(&world, &mut queue).await;
    let flaky = Flaky {
        inner: world.access.clone(),
        fail_adds: true,
        fail_discards: false,
    };

    let result = store_at_boundary(
        &flaky,
        &world.inbox,
        &world.context(),
        &mut queue,
        turn_id(),
    )
    .await;

    assert!(result.is_err());
    assert!(!world.hub.is_read(world.agent, first.id));
    let kept: Vec<_> = queue.items().iter().map(InboxItem::id).collect();
    assert_eq!(kept, vec![first.id.entry_id(), second.id()]);
    let batch = world
        .inbox
        .items(
            &world.context(),
            None,
            std::num::NonZeroUsize::new(5).unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(batch.items.len(), 2);
}

#[tokio::test]
async fn given_a_stored_item_when_the_boundary_runs_again_then_the_next_inbox_read_lists_the_entry_no_more()
 {
    let world = World::new();
    world.start_with_human("start").await;
    let entry = human("once");
    world.hub.deliver_entry(world.agent, entry.clone());
    let mut queue = Queue::new();
    queue_all(&world, &mut queue).await;
    store_at_boundary(
        &world.access,
        &world.inbox,
        &world.context(),
        &mut queue,
        turn_id(),
    )
    .await
    .unwrap();

    let batch = world
        .inbox
        .items(
            &world.context(),
            None,
            std::num::NonZeroUsize::new(5).unwrap(),
        )
        .await
        .unwrap();

    assert!(batch.items.is_empty());
    let projected = world.projected().await;
    assert!(
        projected
            .entries()
            .any(|placed| matches!(placed.entry(), EntryView::Incoming(_))
                && placed.entry().id() == entry.id())
    );
}
