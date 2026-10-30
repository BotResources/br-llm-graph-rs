use br_agentic_contract::{Attachment, DocumentRef, InboxItem, InboxPort, MailDocument, MailItem};

use crate::common::*;

async fn item_of(world: &World, agent: br_llm_messages::AgentId) -> InboxItem {
    world
        .inbox
        .items(&world.context_of(agent), None, limit(5))
        .await
        .unwrap()
        .items
        .remove(0)
}

#[tokio::test]
async fn given_a_mail_with_a_recipient_in_to_when_it_is_sent_then_the_sender_is_the_agent_of_the_call()
 {
    let world = World::new();
    let recipient = agent_id();

    world
        .inbox
        .send(&world.context(), new_mail(&[recipient], &[], "hello"))
        .await
        .unwrap();

    let InboxItem::Mail(MailItem::To { sender, .. }) = item_of(&world, recipient).await else {
        panic!("a to item")
    };
    assert_eq!(sender, world.agent);
}

#[tokio::test]
async fn given_a_mail_when_it_is_sent_then_the_to_item_holds_the_author_name_and_the_source_conversation_of_the_call()
 {
    let world = World::new();
    let recipient = agent_id();
    let mail = new_mail(&[recipient], &[], "hello");
    let (name, source, id) = (mail.author.clone(), mail.source, mail.id);

    world.inbox.send(&world.context(), mail).await.unwrap();

    let InboxItem::Mail(MailItem::To {
        id: item_id,
        author,
        source: item_source,
        subject,
        body,
        ..
    }) = item_of(&world, recipient).await
    else {
        panic!("a to item")
    };
    assert_eq!((item_id, author, item_source), (id, name, source));
    assert_eq!(
        (subject.as_str(), body.as_str()),
        ("hello", "body of hello")
    );
}

#[tokio::test]
async fn given_a_mail_with_to_cc_and_attachments_when_it_is_sent_then_the_to_item_holds_the_recipients_and_the_attachment_names_and_the_cc_item_only_the_id_and_the_subject()
 {
    let world = World::new();
    let (to, cc) = (agent_id(), agent_id());
    let mut mail = new_mail(&[to], &[cc], "report");
    mail.attachments = vec![Attachment::new("a.txt", DocumentRef::new("ref-a"))];
    let id = mail.id;

    world.inbox.send(&world.context(), mail).await.unwrap();

    let InboxItem::Mail(MailItem::To {
        to: seen_to,
        cc: seen_cc,
        attachment_names,
        ..
    }) = item_of(&world, to).await
    else {
        panic!("a to item")
    };
    assert_eq!((seen_to, seen_cc), (vec![to], vec![cc]));
    assert_eq!(attachment_names, ["a.txt"]);
    let InboxItem::Mail(copy) = item_of(&world, cc).await else {
        panic!("a cc item")
    };
    assert_eq!(
        copy,
        MailItem::Cc {
            id,
            subject: "report".to_owned()
        }
    );
}

#[tokio::test]
async fn given_a_mail_without_a_recipient_in_to_when_it_is_sent_then_it_is_refused_with_a_reason_and_nobody_gets_it()
 {
    let world = World::new();
    let cc = agent_id();
    let mail = new_mail(&[], &[cc], "nobody to");
    let id = mail.id;

    let error = world.inbox.send(&world.context(), mail).await.unwrap_err();

    assert!(error.is_refusal());
    assert!(!error.reason().unwrap().as_str().is_empty());
    assert!(world.inbox.mail(id).is_none());
}

#[tokio::test]
async fn given_a_recipient_listed_twice_in_to_and_cc_together_when_the_mail_is_sent_then_it_is_refused_with_a_reason()
 {
    let world = World::new();
    let other = agent_id();
    for mail in [
        new_mail(&[other, other], &[], "twice in to"),
        new_mail(&[agent_id(), other], &[other], "in to and cc"),
        new_mail(&[agent_id()], &[other, other], "twice in cc"),
    ] {
        let id = mail.id;

        let error = world.inbox.send(&world.context(), mail).await.unwrap_err();

        assert!(error.is_refusal());
        assert!(world.inbox.mail(id).is_none());
    }
}

#[tokio::test]
async fn given_an_accepted_mail_when_the_send_returns_then_the_recipient_has_not_had_to_read_it() {
    let world = World::new();
    let recipient = agent_id();
    let mail = new_mail(&[recipient], &[], "hello");
    let id = mail.id;

    world.inbox.send(&world.context(), mail).await.unwrap();

    assert!(!world.inbox.is_read(recipient, id.entry_id()));
    assert!(world.inbox.mail(id).is_some());
}

#[tokio::test]
async fn given_a_mail_whose_id_was_sent_already_when_it_is_sent_again_then_it_is_accepted_and_the_first_mail_stays()
 {
    let world = World::new();
    let recipient = agent_id();
    let first = new_mail(&[recipient], &[], "first");
    let mut second = new_mail(&[recipient, agent_id()], &[], "second");
    second.id = first.id;
    let id = first.id;

    world.inbox.send(&world.context(), first).await.unwrap();
    world.inbox.send(&world.context(), second).await.unwrap();

    let kept = world.inbox.mail(id).unwrap().mail;
    assert_eq!(kept.subject, "first");
    assert_eq!(kept.to, vec![recipient]);
    let items = world
        .inbox
        .items(&world.context_of(recipient), None, limit(5))
        .await
        .unwrap();
    assert_eq!(items.items.len(), 1);
}

#[tokio::test]
async fn given_a_mail_with_attachments_when_a_recipient_in_to_asks_for_the_documents_then_it_gets_each_one_with_the_name_of_its_attachment()
 {
    let world = World::new();
    let recipient = agent_id();
    let mut mail = new_mail(&[recipient], &[], "files");
    mail.attachments = vec![
        Attachment::new("a.txt", DocumentRef::new("ref-a")),
        Attachment::new("b.bin", DocumentRef::new("ref-b")),
    ];
    let id = mail.id;
    world
        .inbox
        .put_document(DocumentRef::new("ref-a"), b"alpha".to_vec());
    world
        .inbox
        .put_document(DocumentRef::new("ref-b"), vec![0, 1, 2]);
    world.inbox.send(&world.context(), mail).await.unwrap();

    let documents = world
        .inbox
        .documents(&world.context_of(recipient), id)
        .await
        .unwrap();

    assert_eq!(
        documents,
        vec![
            MailDocument {
                name: "a.txt".to_owned(),
                content: b"alpha".to_vec()
            },
            MailDocument {
                name: "b.bin".to_owned(),
                content: vec![0, 1, 2]
            },
        ]
    );
}

#[tokio::test]
async fn given_a_mail_without_attachment_when_a_recipient_in_to_asks_for_the_documents_then_it_gets_no_document()
 {
    let world = World::new();
    let recipient = agent_id();
    let mail = new_mail(&[recipient], &[], "plain");
    let id = mail.id;
    world.inbox.send(&world.context(), mail).await.unwrap();

    let documents = world
        .inbox
        .documents(&world.context_of(recipient), id)
        .await
        .unwrap();

    assert!(documents.is_empty());
}

#[tokio::test]
async fn given_an_agent_that_is_not_a_recipient_in_to_when_it_asks_for_the_documents_then_the_request_is_refused_with_a_reason()
 {
    let world = World::new();
    let (to, cc, stranger) = (agent_id(), agent_id(), agent_id());
    let mail = new_mail(&[to], &[cc], "files");
    let id = mail.id;
    world.inbox.send(&world.context(), mail).await.unwrap();

    for agent in [cc, stranger, world.agent] {
        let error = world
            .inbox
            .documents(&world.context_of(agent), id)
            .await
            .unwrap_err();
        assert!(error.is_refusal());
        assert!(!error.reason().unwrap().as_str().is_empty());
    }
}

#[tokio::test]
async fn given_a_mail_that_the_inbox_does_not_know_when_the_documents_are_asked_then_the_request_is_refused()
 {
    let world = World::new();

    let error = world
        .inbox
        .documents(&world.context(), mail_id())
        .await
        .unwrap_err();

    assert!(error.is_refusal());
}

#[tokio::test]
async fn given_a_document_that_the_implementer_cannot_give_when_it_is_asked_then_it_is_a_fault_not_a_refusal()
 {
    let world = World::new();
    let recipient = agent_id();
    let mut mail = new_mail(&[recipient], &[], "files");
    mail.attachments = vec![Attachment::new(
        "lost.txt",
        DocumentRef::new("never-stored"),
    )];
    let id = mail.id;
    world.inbox.send(&world.context(), mail).await.unwrap();

    let error = world
        .inbox
        .documents(&world.context_of(recipient), id)
        .await
        .unwrap_err();

    assert!(!error.is_refusal());
}
