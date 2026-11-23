use br_agentic_contract::{Attachment, DocumentRef, IncomingItem, MailItem};
use br_agentic_harness_contract::MailPort;

use crate::common::*;

async fn send_to(
    world: &World,
    recipient: br_llm_messages::AgentId,
    subject: &str,
) -> Result<br_agentic_contract::MailId, Box<dyn std::error::Error>> {
    let mail = new_mail(&[recipient], &[], subject)?;
    let id = mail.id;
    world.mail.send(&world.context()?, mail).await?;
    Ok(id)
}

#[tokio::test]
async fn given_three_unread_mails_when_the_items_are_read_in_batches_of_two_then_they_come_oldest_first_with_a_cursor()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let first = send_to(&world, recipient, "one").await?;
    let second = send_to(&world, recipient, "two").await?;
    let third = send_to(&world, recipient, "three").await?;
    let context = world.context_of(recipient)?;

    let batch = world.mail.items(&context, None, limit(2)?).await?;
    let ids: Vec<_> = batch.items.iter().map(IncomingItem::id).collect();
    assert_eq!(ids, vec![first.entry_id(), second.entry_id()]);
    let cursor = batch.next.ok_or("a cursor for the next batch")?;

    let rest = world.mail.items(&context, Some(&cursor), limit(2)?).await?;
    let ids: Vec<_> = rest.items.iter().map(IncomingItem::id).collect();
    assert_eq!(ids, vec![third.entry_id()]);
    assert!(rest.next.is_none());
    Ok(())
}

#[tokio::test]
async fn given_a_read_item_when_the_items_are_read_again_then_it_is_not_listed_and_the_list_is_the_truth()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let first = send_to(&world, recipient, "one").await?;
    let second = send_to(&world, recipient, "two").await?;
    let context = world.context_of(recipient)?;

    world.mail.mark_read(&context, first.entry_id()).await?;

    for _ in 0..2 {
        let batch = world.mail.items(&context, None, limit(5)?).await?;
        let ids: Vec<_> = batch.items.iter().map(IncomingItem::id).collect();
        assert_eq!(ids, vec![second.entry_id()]);
    }
    Ok(())
}

#[tokio::test]
async fn given_an_item_already_read_or_an_id_the_inbox_does_not_hold_when_it_is_marked_then_nothing_changes_and_there_is_no_error()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let held = send_to(&world, recipient, "one").await?;
    let context = world.context_of(recipient)?;
    world.mail.mark_read(&context, held.entry_id()).await?;

    world.mail.mark_read(&context, held.entry_id()).await?;
    world.mail.mark_read(&context, entry_id()?).await?;

    let stranger = world.context_of(agent_id()?)?;
    world.mail.mark_read(&stranger, held.entry_id()).await?;
    assert!(world.mail.incoming().is_read(recipient, held.entry_id()));
    assert!(
        !world
            .mail
            .incoming()
            .is_read(stranger.agent, held.entry_id())
    );
    Ok(())
}

#[tokio::test]
async fn given_a_mail_when_it_is_sent_then_the_sender_is_the_agent_identity_of_the_call() -> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    send_to(&world, recipient, "hello").await?;

    let batch = world
        .mail
        .items(&world.context_of(recipient)?, None, limit(5)?)
        .await?;

    let Some(IncomingItem::Mail(MailItem::To { sender, .. })) = batch.items.first() else {
        return Err("a to item".into());
    };
    assert_eq!(*sender, world.agent);
    Ok(())
}

#[tokio::test]
async fn given_a_mail_with_no_recipient_in_to_when_it_is_sent_then_it_is_refused_with_a_reason()
-> Res {
    let world = World::new()?;
    let mail = new_mail(&[], &[agent_id()?], "nobody")?;

    let sent = world.mail.send(&world.context()?, mail).await;

    assert!(matches!(sent, Err(ref error) if error.is_refusal()));
    Ok(())
}

#[tokio::test]
async fn given_a_recipient_listed_twice_in_to_and_cc_when_the_mail_is_sent_then_it_is_refused_with_a_reason()
-> Res {
    let world = World::new()?;
    let twice = agent_id()?;
    let mail = new_mail(&[twice], &[twice], "twice")?;

    let sent = world.mail.send(&world.context()?, mail).await;

    assert!(matches!(sent, Err(ref error) if error.is_refusal()));
    Ok(())
}

#[tokio::test]
async fn given_a_mail_whose_id_was_sent_already_when_it_is_sent_again_then_it_is_accepted_and_changes_nothing()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let mail = new_mail(&[recipient], &[], "once")?;

    world.mail.send(&world.context()?, mail.clone()).await?;
    world.mail.send(&world.context()?, mail).await?;

    let batch = world
        .mail
        .items(&world.context_of(recipient)?, None, limit(5)?)
        .await?;
    assert_eq!(batch.items.len(), 1);
    Ok(())
}

async fn send_with_attachment(
    world: &World,
    to: br_llm_messages::AgentId,
    cc: Option<br_llm_messages::AgentId>,
) -> Result<br_agentic_contract::MailId, Box<dyn std::error::Error>> {
    let cc: Vec<_> = cc.into_iter().collect();
    let mut mail = new_mail(&[to], &cc, "files")?;
    mail.attachments = vec![
        Attachment::new("a.txt", DocumentRef::new("doc-a")),
        Attachment::new("b.txt", DocumentRef::new("doc-b")),
    ];
    let id = mail.id;
    world
        .mail
        .put_document(DocumentRef::new("doc-a"), b"A".to_vec());
    world
        .mail
        .put_document(DocumentRef::new("doc-b"), b"B".to_vec());
    world.mail.send(&world.context()?, mail).await?;
    Ok(id)
}

#[tokio::test]
async fn given_a_mail_with_attachments_when_a_recipient_in_to_asks_for_the_documents_then_it_gets_them()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let id = send_with_attachment(&world, recipient, None).await?;

    let documents = world
        .mail
        .documents(&world.context_of(recipient)?, id)
        .await?;

    let held: Vec<_> = documents
        .iter()
        .map(|document| (document.name.as_str(), document.content.as_slice()))
        .collect();
    assert_eq!(
        held,
        vec![("a.txt", b"A".as_slice()), ("b.txt", b"B".as_slice())]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_mail_without_attachment_when_the_documents_are_asked_then_there_is_no_document()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let id = send_to(&world, recipient, "plain").await?;

    let documents = world
        .mail
        .documents(&world.context_of(recipient)?, id)
        .await?;

    assert!(documents.is_empty());
    Ok(())
}

#[tokio::test]
async fn given_a_mail_when_a_cc_recipient_a_stranger_or_an_unknown_id_asks_for_the_documents_then_it_is_refused_with_a_reason()
-> Res {
    let world = World::new()?;
    let (to, cc) = (agent_id()?, agent_id()?);
    let id = send_with_attachment(&world, to, Some(cc)).await?;

    for asker in [cc, agent_id()?] {
        let refused = world.mail.documents(&world.context_of(asker)?, id).await;
        assert!(matches!(refused, Err(ref error) if error.is_refusal()));
    }
    let unknown = world
        .mail
        .documents(&world.context_of(to)?, mail_id()?)
        .await;
    assert!(matches!(unknown, Err(ref error) if error.is_refusal()));
    Ok(())
}
