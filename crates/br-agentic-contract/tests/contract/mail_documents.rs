use br_agentic_contract::memory::Call;
use br_agentic_contract::{Attachment, DocumentRef, MailDocument, mail_documents};
use br_llm_messages::{AgentMail, TurnEntry};

use crate::common::*;

fn documents_calls(world: &World) -> usize {
    world
        .record
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::InboxDocuments { .. }))
        .count()
}

async fn mail_with_file(world: &World) -> br_agentic_contract::MailId {
    let mut mail = new_mail(&[world.agent], &[], "files");
    mail.attachments = vec![Attachment::new("a.txt", DocumentRef::new("ref"))];
    let id = mail.id;
    world
        .inbox
        .put_document(DocumentRef::new("ref"), b"data".to_vec());
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    id
}

fn mail_entry_for(id: br_agentic_contract::MailId) -> br_llm_messages::AgentMail {
    AgentMail::new(
        id.entry_id(),
        agent_id(),
        author("sender"),
        conversation_id(),
        vec![text_block("files")],
    )
    .unwrap()
}

#[tokio::test]
async fn given_a_mail_whose_entry_the_conversation_holds_as_an_incoming_entry_when_the_documents_are_asked_then_the_inbox_gives_them()
 {
    let world = World::new();
    let id = mail_with_file(&world).await;
    let mut session = world.session().await;
    session
        .add_input_turn(
            br_llm_messages::InputTurn::new(turn_id(), vec![mail_entry_for(id).into()]).unwrap(),
        )
        .await
        .unwrap();

    let documents = mail_documents(&session, &world.inbox, &world.context(), id)
        .await
        .unwrap();

    assert_eq!(
        documents,
        vec![MailDocument {
            name: "a.txt".to_owned(),
            content: b"data".to_vec()
        }]
    );
}

#[tokio::test]
async fn given_a_mail_whose_entry_is_an_incoming_entry_in_an_ai_turn_when_the_documents_are_asked_then_the_inbox_gives_them()
 {
    let world = World::new();
    let id = mail_with_file(&world).await;
    let mut session = world.session().await;
    session
        .add_entry(&world.ai(turn_id()), TurnEntry::from(mail_entry_for(id)))
        .await
        .unwrap();

    let documents = mail_documents(&session, &world.inbox, &world.context(), id)
        .await
        .unwrap();

    assert_eq!(documents.len(), 1);
}

#[tokio::test]
async fn given_a_mail_id_whose_entry_the_conversation_does_not_hold_when_the_documents_are_asked_then_the_answer_is_a_refusal_for_the_agent_and_the_inbox_is_not_asked()
 {
    let world = World::new();
    let id = mail_with_file(&world).await;
    let session = world.session().await;

    let error = mail_documents(&session, &world.inbox, &world.context(), id)
        .await
        .unwrap_err();

    assert!(error.is_refusal());
    assert_eq!(documents_calls(&world), 0);
}

#[tokio::test]
async fn given_an_id_that_the_conversation_holds_as_another_kind_of_entry_when_the_documents_are_asked_then_the_answer_is_a_refusal()
 {
    let world = World::new();
    let mut session = world.session().await;
    let entry = entry_id();
    session
        .add_entry(
            &world.ai(turn_id()),
            TurnEntry::from(response_text(entry, "not a mail")),
        )
        .await
        .unwrap();

    let error = mail_documents(
        &session,
        &world.inbox,
        &world.context(),
        br_agentic_contract::MailId::new(entry),
    )
    .await
    .unwrap_err();

    assert!(error.is_refusal());
}
