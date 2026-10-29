use std::num::NonZeroUsize;

use br_agentic_contract::{Attachment, DocumentRef, InboxError, InboxPort, MailDocument};

use crate::common::*;

fn limit(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[tokio::test]
async fn given_a_mail_without_a_recipient_in_to_when_it_is_sent_then_it_is_refused_with_a_reason_and_not_stored()
 {
    let world = World::new();
    let mail = new_mail(&[], &[world.agent], "nobody");

    let error = world
        .inbox
        .send(&world.context_of(agent_id()), mail)
        .await
        .unwrap_err();

    assert!(matches!(error, InboxError::MailRefused(reason) if !reason.as_str().is_empty()));
    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();
    assert!(batch.items.is_empty());
}

#[tokio::test]
async fn given_a_mail_with_a_recipient_listed_twice_when_it_is_sent_then_it_is_refused_with_a_reason_in_to_and_across_to_and_cc()
 {
    let world = World::new();
    let twice = agent_id();
    let sender = world.context_of(agent_id());

    let in_to = world
        .inbox
        .send(&sender, new_mail(&[twice, twice], &[], "x"))
        .await
        .unwrap_err();
    let across = world
        .inbox
        .send(&sender, new_mail(&[twice], &[twice], "x"))
        .await
        .unwrap_err();

    assert!(matches!(in_to, InboxError::MailRefused(_)));
    assert!(matches!(across, InboxError::MailRefused(_)));
}

#[tokio::test]
async fn given_a_mail_already_sent_when_it_is_sent_again_with_the_same_id_then_nothing_changes() {
    let world = World::new();
    let sender = world.context_of(agent_id());
    let mail = new_mail(&[world.agent], &[], "once");
    world.inbox.send(&sender, mail.clone()).await.unwrap();
    world
        .inbox
        .mark_read(&world.context(), mail.id)
        .await
        .unwrap();

    world.inbox.send(&sender, mail).await.unwrap();

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();
    assert!(batch.items.is_empty());
}

#[tokio::test]
async fn given_a_mail_with_attachments_when_the_documents_are_asked_then_only_the_sender_and_recipients_see_them()
 {
    let world = World::new();
    let sender = agent_id();
    let mut mail = new_mail(&[world.agent], &[], "docs");
    mail.attachments = vec![Attachment::new("a.txt", DocumentRef::new("doc-1"))];
    world
        .hub
        .put_document(DocumentRef::new("doc-1"), b"content".to_vec());
    let id = mail.id;
    world
        .inbox
        .send(&world.context_of(sender), mail)
        .await
        .unwrap();

    let seen = world.inbox.documents(&world.context(), id).await.unwrap();
    let outsider = world
        .inbox
        .documents(&world.context_of(agent_id()), id)
        .await
        .unwrap();

    assert_eq!(
        seen,
        vec![MailDocument {
            name: "a.txt".to_owned(),
            content: b"content".to_vec()
        }]
    );
    assert!(outsider.is_empty());
}
