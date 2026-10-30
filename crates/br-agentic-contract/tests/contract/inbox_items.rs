use std::time::Duration;

use br_agentic_contract::memory::{Call, MemoryInbox};
use br_agentic_contract::{InboxItem, InboxPort, MailItem};
use br_llm_messages::EntryId;
use tokio::time::timeout;

use crate::common::*;

async fn all_ids(inbox: &MemoryInbox, world: &World, size: usize) -> Vec<EntryId> {
    let context = world.context();
    let mut ids = Vec::new();
    let mut after = None;
    loop {
        let batch = inbox
            .items(&context, after.as_ref(), limit(size))
            .await
            .unwrap();
        ids.extend(batch.items.iter().map(InboxItem::id));
        match batch.next {
            Some(cursor) => after = Some(cursor),
            None => return ids,
        }
    }
}

fn deliver(world: &World, count: usize) -> Vec<EntryId> {
    (0..count)
        .map(|n| {
            let entry = human(&format!("m{n}"));
            let id = entry.id();
            world.inbox.deliver_entry(world.agent, entry);
            id
        })
        .collect()
}

#[tokio::test]
async fn given_items_for_an_agent_when_the_inbox_is_read_then_they_come_oldest_first() {
    let world = World::new();
    let ids = deliver(&world, 4);

    assert_eq!(all_ids(&world.inbox, &world, 10).await, ids);
}

#[tokio::test]
async fn given_more_items_than_the_bound_when_the_inbox_is_read_then_a_batch_has_at_most_the_bound_and_a_cursor_for_the_next()
 {
    let world = World::new();
    let ids = deliver(&world, 5);

    let first = world
        .inbox
        .items(&world.context(), None, limit(2))
        .await
        .unwrap();

    assert_eq!(first.items.len(), 2);
    assert!(first.next.is_some());
    let second = world
        .inbox
        .items(&world.context(), first.next.as_ref(), limit(2))
        .await
        .unwrap();
    assert_eq!(
        second.items.iter().map(InboxItem::id).collect::<Vec<_>>(),
        ids[2..4]
    );
}

#[tokio::test]
async fn given_a_bound_of_one_when_the_inbox_is_read_then_each_batch_gives_one_item_in_order() {
    let world = World::new();
    let ids = deliver(&world, 3);

    assert_eq!(all_ids(&world.inbox, &world, 1).await, ids);
}

#[tokio::test]
async fn given_the_last_items_when_the_inbox_is_read_then_the_last_batch_gives_no_cursor() {
    let world = World::new();
    deliver(&world, 2);

    let exact = world
        .inbox
        .items(&world.context(), None, limit(2))
        .await
        .unwrap();
    let larger = world
        .inbox
        .items(&world.context(), None, limit(9))
        .await
        .unwrap();

    assert!(exact.next.is_none());
    assert!(larger.next.is_none());
}

#[tokio::test]
async fn given_no_item_when_the_inbox_is_read_then_the_batch_is_empty_and_has_no_cursor() {
    let world = World::new();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(3))
        .await
        .unwrap();

    assert!(batch.items.is_empty() && batch.next.is_none());
}

#[tokio::test]
async fn given_a_cursor_when_an_item_is_marked_as_read_or_arrives_in_the_meantime_then_the_cursor_stays_valid()
 {
    let world = World::new();
    let ids = deliver(&world, 4);
    let first = world
        .inbox
        .items(&world.context(), None, limit(2))
        .await
        .unwrap();
    world
        .inbox
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();
    world
        .inbox
        .mark_read(&world.context(), ids[2])
        .await
        .unwrap();
    let late = human("late");
    let late_id = late.id();
    world.inbox.deliver_entry(world.agent, late);

    let rest = world
        .inbox
        .items(&world.context(), first.next.as_ref(), limit(10))
        .await
        .unwrap();

    assert_eq!(
        rest.items.iter().map(InboxItem::id).collect::<Vec<_>>(),
        [ids[3], late_id]
    );
}

#[tokio::test]
async fn given_an_inbox_read_when_it_is_read_again_then_the_list_it_gives_is_the_truth() {
    let world = World::new();
    let ids = deliver(&world, 2);
    let first = all_ids(&world.inbox, &world, 10).await;
    world
        .inbox
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    let second = all_ids(&world.inbox, &world, 10).await;

    assert_eq!(first, ids);
    assert_eq!(second, [ids[1]]);
}

#[tokio::test]
async fn given_items_for_two_agents_when_one_reads_then_it_gets_only_its_own() {
    let world = World::new();
    let other = agent_id();
    let mine = deliver(&world, 1);
    world.inbox.deliver_entry(other, human("not mine"));

    assert_eq!(all_ids(&world.inbox, &world, 10).await, mine);
}

#[tokio::test]
async fn given_an_item_read_by_one_instance_when_another_instance_of_the_same_agent_reads_then_it_gets_the_items_of_the_agent_whichever_instance_read_before()
 {
    let world = World::new();
    let ids = deliver(&world, 2);
    world
        .inbox
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    let other_instance = world.context();
    let batch = world
        .inbox
        .items(&other_instance, None, limit(10))
        .await
        .unwrap();

    assert_eq!(
        batch.items.iter().map(InboxItem::id).collect::<Vec<_>>(),
        [ids[1]]
    );
}

#[tokio::test]
async fn given_a_mail_a_human_message_a_notification_and_a_notice_when_the_inbox_is_read_then_all_four_kinds_are_items()
 {
    let world = World::new();
    let sender = agent_id();
    world
        .inbox
        .deliver_mail(sender, new_mail(&[world.agent], &[], "hello"))
        .unwrap();
    world.inbox.deliver_entry(world.agent, human("hi"));
    world.inbox.deliver_entry(world.agent, notification());
    world.inbox.deliver_entry(world.agent, notice("fyi"));

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    assert_eq!(batch.items.len(), 4);
    assert!(matches!(
        batch.items[0],
        InboxItem::Mail(MailItem::To { .. })
    ));
}

#[tokio::test]
async fn given_an_unread_item_when_the_caller_waits_then_the_wait_returns_at_once() {
    let world = World::new();
    deliver(&world, 1);

    let waited = timeout(Duration::from_secs(1), world.inbox.wait(&world.context())).await;

    assert!(waited.unwrap().is_ok());
}

#[tokio::test]
async fn given_an_item_that_a_read_already_gave_and_that_is_still_unread_when_the_caller_waits_then_the_wait_returns_at_once()
 {
    let world = World::new();
    deliver(&world, 1);
    world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    let waited = timeout(Duration::from_secs(1), world.inbox.wait(&world.context())).await;

    assert!(waited.unwrap().is_ok());
}

#[tokio::test]
async fn given_no_unread_item_when_the_caller_waits_then_the_wait_returns_when_an_item_arrives_and_gives_nothing_else()
 {
    let world = World::new();
    let context = world.context();
    let waiting = tokio::spawn({
        let inbox = world.inbox.clone();
        async move { inbox.wait(&context).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished());

    deliver(&world, 1);

    let outcome = timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
    assert!(outcome.is_ok());
}

#[tokio::test]
async fn given_a_wait_when_an_item_arrives_for_another_agent_then_the_wait_goes_on() {
    let world = World::new();
    let context = world.context();
    let waiting = tokio::spawn({
        let inbox = world.inbox.clone();
        async move { inbox.wait(&context).await }
    });

    world
        .inbox
        .deliver_entry(agent_id(), human("for someone else"));
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(!waiting.is_finished());
    waiting.abort();
}

#[tokio::test]
async fn given_a_wait_that_is_dropped_when_an_item_arrives_then_nothing_is_lost() {
    let world = World::new();
    let context = world.context();
    let dropped = timeout(Duration::from_millis(30), world.inbox.wait(&context)).await;
    assert!(dropped.is_err());
    let ids = deliver(&world, 1);

    assert_eq!(all_ids(&world.inbox, &world, 10).await, ids);
}

#[tokio::test]
async fn given_a_mail_a_human_message_a_notification_and_a_notice_when_each_is_marked_as_read_then_none_is_listed_any_more()
 {
    let world = World::new();
    world
        .inbox
        .deliver_mail(agent_id(), new_mail(&[world.agent], &[], "hello"))
        .unwrap();
    world.inbox.deliver_entry(world.agent, human("hi"));
    world.inbox.deliver_entry(world.agent, notification());
    world.inbox.deliver_entry(world.agent, notice("fyi"));
    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    for item in &batch.items {
        world
            .inbox
            .mark_read(&world.context(), item.id())
            .await
            .unwrap();
        assert!(world.inbox.is_read(world.agent, item.id()));
    }

    let rest = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();
    assert!(rest.items.is_empty());
}

#[tokio::test]
async fn given_an_item_already_read_when_it_is_marked_again_then_nothing_changes_and_it_is_not_an_error()
 {
    let world = World::new();
    let ids = deliver(&world, 2);
    world
        .inbox
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    world
        .inbox
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    assert_eq!(all_ids(&world.inbox, &world, 10).await, [ids[1]]);
}

#[tokio::test]
async fn given_an_id_that_the_inbox_does_not_hold_for_the_agent_when_it_is_marked_then_nothing_changes_and_it_is_not_an_error()
 {
    let world = World::new();
    let ids = deliver(&world, 1);
    let other = agent_id();
    world.inbox.deliver_entry(other, human("theirs"));
    let theirs = world
        .inbox
        .items(&world.context_of(other), None, limit(1))
        .await
        .unwrap()
        .items[0]
        .id();

    world
        .inbox
        .mark_read(&world.context(), entry_id())
        .await
        .unwrap();
    world
        .inbox
        .mark_read(&world.context(), theirs)
        .await
        .unwrap();

    assert!(!world.inbox.is_read(other, theirs));
    assert_eq!(all_ids(&world.inbox, &world, 10).await, ids);
}

#[tokio::test]
async fn given_a_mark_when_the_item_is_read_then_the_only_change_is_its_read_state_the_mail_stays_known()
 {
    let world = World::new();
    let mail = new_mail(&[world.agent], &[], "kept");
    let id = mail.id;
    world.inbox.deliver_mail(agent_id(), mail).unwrap();

    world
        .inbox
        .mark_read(&world.context(), id.entry_id())
        .await
        .unwrap();

    assert!(world.inbox.mail(id).is_some());
}

#[tokio::test]
async fn given_the_inbox_calls_when_the_test_reads_the_record_then_they_carry_the_context_of_the_caller()
 {
    let world = World::new();
    let context = world.context();
    world.inbox.items(&context, None, limit(3)).await.unwrap();

    let recorded = world.record.calls();
    assert!(
        matches!(&recorded[0], Call::InboxItems { context: c, limit: l, .. } if *c == context && l.get() == 3)
    );
}
