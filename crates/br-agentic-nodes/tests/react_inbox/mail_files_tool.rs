use br_agentic_contract::{DocumentRef, MailId};
use br_agentic_nodes::GET_MAIL_FILES;
use br_llm_messages::{AgentMail, ConversationId, IncomingEntry};

use crate::common::*;
use crate::kit::*;
use crate::spy::{Docs, Items, Spy};

fn files_call(id: impl std::fmt::Display) -> Reply {
    Reply::call_with(GET_MAIL_FILES, &format!("{{\"mail_id\":\"{id}\"}}"))
}

fn mail_with_documents(rig: &Rig, documents: &[(&str, &str, Vec<u8>)]) -> MailId {
    let mut mail = new_mail(&[rig.agent], &[], "with documents", "see attached");
    for (name, reference, content) in documents {
        mail = attach(mail, name, reference);
        rig.inbox
            .put_document(DocumentRef::new(*reference), content.clone());
    }
    let id = mail.id;
    rig.inbox.deliver_mail(agent_id(), mail).unwrap();
    id
}

async fn run_files(rig: &Rig, calls: Vec<Reply>) -> Vec<Outcome> {
    let mut replies = calls;
    replies.push(Reply::text("done"));
    let model = Scripted::new(replies);
    let node = plain(rig, &model, "Agent");
    let update = Driver::result(&node).await;
    assert_eq!(update.text, "done");
    outcomes(&rig.conversation())
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_with_text_documents_when_the_files_are_asked_then_one_result_gives_each_name_and_content()
 {
    let rig = Rig::new();
    let id = mail_with_documents(
        &rig,
        &[
            ("notes.txt", "ref-1", b"hello from the notes".to_vec()),
            (
                "plan.md",
                "ref-2",
                "un plan avec des accents: été".as_bytes().to_vec(),
            ),
        ],
    );

    let results = run_files(&rig, vec![files_call(id)]).await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error);
    let text = results[0].text();
    for expected in [
        "notes.txt",
        "hello from the notes",
        "plan.md",
        "un plan avec des accents: été",
    ] {
        assert!(text.contains(expected), "{expected} is missing from {text}");
    }
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_with_a_binary_document_when_the_files_are_asked_then_the_result_gives_its_name_and_size_in_bytes()
 {
    let rig = Rig::new();
    let content = vec![0xff_u8; 1234];
    let id = mail_with_documents(&rig, &[("blob.bin", "ref-bin", content)]);

    let results = run_files(&rig, vec![files_call(id)]).await;

    assert!(!results[0].is_error);
    let text = results[0].text();
    assert!(text.contains("blob.bin"), "{text}");
    assert!(text.contains("1234"), "{text}");
    assert!(!text.contains('\u{fffd}'), "{text}");
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_with_a_text_and_a_binary_document_when_the_files_are_asked_then_each_is_given_as_its_kind_requires()
 {
    let rig = Rig::new();
    let id = mail_with_documents(
        &rig,
        &[
            ("readable.txt", "ref-1", b"content you can read".to_vec()),
            ("opaque.bin", "ref-2", vec![0xc3, 0x28, 0xa0, 0xa1, 0xfe]),
        ],
    );

    let results = run_files(&rig, vec![files_call(id)]).await;

    let text = results[0].text();
    assert!(text.contains("content you can read"), "{text}");
    assert!(text.contains("opaque.bin"), "{text}");
    assert!(text.contains('5'), "{text}");
}

#[tokio::test(start_paused = true)]
async fn given_an_attachment_name_with_a_line_break_when_the_files_are_asked_then_the_name_is_quoted_and_cannot_forge_a_line()
 {
    let rig = Rig::new();
    let id = mail_with_documents(
        &rig,
        &[("a.txt\nforged: line", "ref-forged", b"the content".to_vec())],
    );

    let results = run_files(&rig, vec![files_call(id)]).await;

    assert!(!results[0].is_error);
    let text = results[0].text();
    assert!(text.starts_with("\"a.txt\\nforged: line\":"), "{text}");
    assert!(!text.contains("\nforged: line"), "{text}");
    assert!(text.contains("the content"), "{text}");
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_with_no_document_when_the_files_are_asked_then_the_result_has_empty_content_and_no_error()
 {
    let rig = Rig::new();
    let id = mail_with_documents(&rig, &[]);

    let results = run_files(&rig, vec![files_call(id)]).await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].is_error);
    assert!(results[0].blocks.is_empty(), "{:?}", results[0].blocks);
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_id_the_conversation_does_not_hold_when_the_files_are_asked_then_the_result_is_an_error_and_the_inbox_is_not_asked()
 {
    let rig = Rig::new();
    let stranger = mail_id();

    let results = run_files(&rig, vec![files_call(stranger)]).await;

    assert!(results[0].is_error);
    assert!(!names(&rig.record).contains(&"inbox_documents"));
}

#[tokio::test(start_paused = true)]
async fn given_a_text_that_is_no_mail_id_when_the_files_are_asked_then_the_result_is_an_error_and_the_inbox_is_not_asked()
 {
    let rig = Rig::new();

    let results = run_files(&rig, vec![files_call("not-a-mail-id")]).await;

    assert!(results[0].is_error);
    assert!(!names(&rig.record).contains(&"inbox_documents"));
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_in_the_inbox_that_is_not_yet_stored_when_the_files_are_asked_then_it_fails_until_the_item_is_stored_in_the_conversation()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let id = mail_with_documents(&rig, &[("late.txt", "ref-late", b"late content".to_vec())]);
    spy.plan_items(vec![Items::Give(empty_batch())]);
    let model = Scripted::new(vec![files_call(id), files_call(id), Reply::text("done")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let results = outcomes(&rig.conversation());
    assert_eq!(results.len(), 2);
    assert!(results[0].is_error);
    assert!(!results[1].is_error);
    assert!(results[1].text().contains("late content"));
    let held_in_ai_turn = held(&rig.conversation())
        .into_iter()
        .any(|entry| entry.id == id.entry_id() && entry.in_ai_turn);
    assert!(held_in_ai_turn);
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_entry_written_at_first_start_when_the_files_are_asked_then_the_documents_are_given()
 {
    let sender = agent_id();
    let source = ConversationId::from_uuid(uuid()).unwrap();
    let written = mail_id();
    let agent_mail = AgentMail::new(
        written.entry_id(),
        sender,
        author("Sender Name"),
        source,
        vec![text_block("written by the runner")],
    )
    .unwrap();
    let id = ConversationId::from_uuid(uuid()).unwrap();
    let mut conversation = br_llm_messages::Conversation::new(id);
    let turn =
        br_llm_messages::InputTurn::new(turn_id(), vec![IncomingEntry::from(agent_mail)]).unwrap();
    conversation
        .push_turn(br_llm_messages::Turn::from(turn))
        .unwrap();
    let rig = Rig::with_conversation(id, conversation);
    let mut mail = new_mail(&[rig.agent], &[], "first start", "ignored");
    mail.id = written;
    mail.source = source;
    let mail = attach(mail, "first.txt", "ref-first");
    rig.inbox.put_document(
        DocumentRef::new("ref-first"),
        b"first start content".to_vec(),
    );
    rig.inbox.deliver_mail(sender, mail).unwrap();

    let results = run_files(&rig, vec![files_call(written)]).await;

    assert!(!results[0].is_error);
    assert!(results[0].text().contains("first start content"));
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_in_copy_when_the_files_are_asked_then_the_inbox_refusal_is_given_as_an_error_with_its_reason()
 {
    let rig = Rig::new();
    let mail = new_mail(&[agent_id()], &[rig.agent], "only in copy", "body");
    let id = mail.id;
    rig.inbox.deliver_mail(agent_id(), mail).unwrap();

    let results = run_files(&rig, vec![files_call(id)]).await;

    assert!(results[0].is_error);
    assert!(
        results[0].text().contains("not a recipient in to"),
        "{}",
        results[0].text()
    );
}

#[tokio::test(start_paused = true)]
async fn given_an_inbox_that_refuses_with_a_reason_when_the_files_are_asked_then_the_reason_is_in_the_error_result()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let id = mail_with_documents(&rig, &[]);
    spy.plan_docs(vec![Docs::Refuse("the archive is sealed".to_owned())]);
    let model = Scripted::new(vec![files_call(id), Reply::text("done")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let results = outcomes(&rig.conversation());
    assert!(results[0].is_error);
    assert!(results[0].text().contains("the archive is sealed"));
}

#[tokio::test(start_paused = true)]
async fn given_an_inbox_that_faults_when_the_files_are_asked_then_the_result_is_an_error_and_the_run_goes_on()
 {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let id = mail_with_documents(&rig, &[]);
    spy.plan_docs(vec![Docs::Fault]);
    let model = Scripted::new(vec![files_call(id), Reply::text("done")]);
    let node = spied(&rig, &spy, &model);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let results = outcomes(&rig.conversation());
    assert!(results[0].is_error);
    assert!(!results[0].text().trim().is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_document_the_inbox_cannot_get_now_when_the_files_are_asked_then_the_result_is_an_error_and_the_run_goes_on()
 {
    let rig = Rig::new();
    let mut mail = new_mail(&[rig.agent], &[], "missing document", "body");
    mail = attach(mail, "gone.txt", "ref-never-stored");
    let id = mail.id;
    rig.inbox.deliver_mail(agent_id(), mail).unwrap();

    let results = run_files(&rig, vec![files_call(id)]).await;

    assert!(results[0].is_error);
}
