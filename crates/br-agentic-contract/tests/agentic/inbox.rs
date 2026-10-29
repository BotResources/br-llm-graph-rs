use std::num::NonZeroUsize;
use std::time::Duration;

use br_agentic_contract::{
    Attachment, DocumentRef, InboxItem, InboxPort, Incoming, MailItem, ThreadAccess,
};

use crate::common::*;

fn limit(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[tokio::test]
async fn given_a_mail_sent_to_an_agent_when_the_inbox_is_read_then_a_mail_item_with_the_recipients_subject_body_and_attachment_names_is_listed()
 {
    let world = World::new();
    let sender = agent_id();
    let other = agent_id();
    let mut mail = new_mail(&[world.agent, other], &[], "plan");
    mail.attachments = vec![Attachment::new("a.txt", DocumentRef::new("doc-1"))];
    let id = mail.id;
    world
        .inbox
        .send(&world.context_of(sender), mail)
        .await
        .unwrap();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    assert_eq!(batch.items.len(), 1);
    match mail_to(&batch.items[0]) {
        MailItem::To {
            id: got,
            sender: from,
            to,
            subject,
            body,
            attachment_names,
            ..
        } => {
            assert_eq!(*got, id);
            assert_eq!(*from, sender);
            assert_eq!(to, &vec![world.agent, other]);
            assert_eq!(subject, "plan");
            assert_eq!(body, "body of plan");
            assert_eq!(attachment_names, &vec!["a.txt".to_owned()]);
        }
        MailItem::Cc { .. } => panic!("the recipient is in to"),
    }
    assert_eq!(batch.next, None);
}

#[tokio::test]
async fn given_a_mail_sent_in_copy_when_the_inbox_is_read_then_only_the_mail_id_and_subject_are_listed()
 {
    let world = World::new();
    let mail = new_mail(&[agent_id()], &[world.agent], "fyi");
    let id = mail.id;
    world
        .inbox
        .send(&world.context_of(agent_id()), mail)
        .await
        .unwrap();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    assert_eq!(
        mail_to(&batch.items[0]),
        &MailItem::Cc {
            id,
            subject: "fyi".to_owned()
        }
    );
}

#[tokio::test]
async fn given_mails_and_entries_when_the_inbox_is_read_then_the_items_come_oldest_first() {
    let world = World::new();
    let sender = world.context_of(agent_id());
    let first = new_mail(&[world.agent], &[], "one");
    let entry = human("two");
    let third = new_mail(&[world.agent], &[], "three");
    world.inbox.send(&sender, first.clone()).await.unwrap();
    world.hub.deliver_entry(world.agent, entry.clone());
    world.inbox.send(&sender, third.clone()).await.unwrap();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    let ids: Vec<_> = batch.items.iter().map(InboxItem::id).collect();
    assert_eq!(
        ids,
        vec![first.id.entry_id(), entry.id(), third.id.entry_id()]
    );
}

#[tokio::test]
async fn given_more_items_than_the_bound_when_batches_are_read_then_each_batch_gives_a_cursor_for_the_next_until_the_list_ends()
 {
    let world = World::new();
    let sender = world.context_of(agent_id());
    let mut ids = Vec::new();
    for subject in ["a", "b", "c", "d", "e"] {
        let mail = new_mail(&[world.agent], &[], subject);
        ids.push(mail.id.entry_id());
        world.inbox.send(&sender, mail).await.unwrap();
    }

    let first = world
        .inbox
        .items(&world.context(), None, limit(2))
        .await
        .unwrap();
    let second = world
        .inbox
        .items(&world.context(), first.next.as_ref(), limit(2))
        .await
        .unwrap();
    let third = world
        .inbox
        .items(&world.context(), second.next.as_ref(), limit(2))
        .await
        .unwrap();

    let all: Vec<_> = [first.clone(), second.clone(), third.clone()]
        .iter()
        .flat_map(|batch| batch.items.iter().map(InboxItem::id))
        .collect();
    assert_eq!(all, ids);
    assert!(first.next.is_some() && second.next.is_some());
    assert_eq!(third.next, None);
}

#[tokio::test]
async fn given_a_cursor_when_an_item_is_marked_as_read_or_stored_between_batches_then_the_cursor_stays_valid_and_no_item_is_skipped_or_repeated()
 {
    let world = World::new();
    let sender_agent = agent_id();
    world.hub.set_author(sender_agent, author("sender"));
    let sender = world.context_of(sender_agent);
    let mails: Vec<_> = ["a", "b", "c", "d"]
        .iter()
        .map(|subject| new_mail(&[world.agent], &[], subject))
        .collect();
    for mail in &mails {
        world.inbox.send(&sender, mail.clone()).await.unwrap();
    }
    let first = world
        .inbox
        .items(&world.context(), None, limit(2))
        .await
        .unwrap();
    world
        .inbox
        .mark_read(&world.context(), mails[0].id)
        .await
        .unwrap();
    world.start_with_human("x").await;
    world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Mail(match &first.items[1] {
                InboxItem::Mail(item) => item.clone(),
                InboxItem::Entry(_) => unreachable!(),
            })],
        )
        .await
        .unwrap();

    let second = world
        .inbox
        .items(&world.context(), first.next.as_ref(), limit(10))
        .await
        .unwrap();

    let ids: Vec<_> = second.items.iter().map(InboxItem::id).collect();
    assert_eq!(ids, vec![mails[2].id.entry_id(), mails[3].id.entry_id()]);
}

#[tokio::test]
async fn given_two_agents_when_each_reads_the_inbox_then_each_gets_only_the_items_of_its_own_identity()
 {
    let world = World::new();
    let other = agent_id();
    let sender = world.context_of(agent_id());
    world
        .inbox
        .send(&sender, new_mail(&[world.agent], &[], "mine"))
        .await
        .unwrap();
    world
        .inbox
        .send(&sender, new_mail(&[other], &[], "yours"))
        .await
        .unwrap();

    let mine = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();
    let yours = world
        .inbox
        .items(&world.context_of(other), None, limit(10))
        .await
        .unwrap();

    assert_eq!(mail_to(&mine.items[0]).subject(), "mine");
    assert_eq!(mail_to(&yours.items[0]).subject(), "yours");
    assert_eq!(mine.items.len() + yours.items.len(), 2);
}

#[tokio::test]
async fn given_an_entry_the_projected_thread_holds_when_the_inbox_is_read_then_it_is_not_listed() {
    let world = World::new();
    let entry = human("stored");
    world.hub.deliver_entry(world.agent, entry.clone());
    world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Entry(entry)],
        )
        .await
        .unwrap();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    assert!(batch.items.is_empty());
}

#[tokio::test]
async fn given_a_mail_marked_as_read_when_the_inbox_is_read_then_it_is_not_listed_and_nothing_else_changed()
 {
    let world = World::new();
    let sender = world.context_of(agent_id());
    let kept = new_mail(&[world.agent], &[], "kept");
    let read = new_mail(&[world.agent], &[], "read");
    world.inbox.send(&sender, kept.clone()).await.unwrap();
    world.inbox.send(&sender, read.clone()).await.unwrap();

    world
        .inbox
        .mark_read(&world.context(), read.id)
        .await
        .unwrap();
    world
        .inbox
        .mark_read(&world.context(), read.id)
        .await
        .unwrap();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    assert_eq!(batch.items[0].id(), kept.id.entry_id());
    assert!(world.hub.is_read(world.agent, read.id));
    assert!(!world.hub.is_read(world.agent, kept.id));
}

#[tokio::test]
async fn given_a_waiting_caller_when_an_item_arrives_for_the_agent_then_the_wait_gives_that_item_and_ignores_other_agents()
 {
    let world = World::new();
    let waiting = world.inbox.clone();
    let context = world.context();
    let task = tokio::spawn(async move { waiting.wait(&context).await.unwrap() });
    tokio::time::sleep(Duration::from_millis(20)).await;
    let sender = world.context_of(agent_id());
    world
        .inbox
        .send(&sender, new_mail(&[agent_id()], &[], "not for me"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!task.is_finished());
    let mail = new_mail(&[world.agent], &[], "for me");

    world.inbox.send(&sender, mail.clone()).await.unwrap();

    let item = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.id(), mail.id.entry_id());
}
