use br_agentic_contract::{Attachment, DocumentRef, InboxItem, InboxPort};
use std::num::NonZeroUsize;

use crate::common::*;

async fn mails_for(rig: &Rig, agent: br_llm_messages::AgentId) -> Vec<String> {
    let batch = rig
        .hub
        .inbox()
        .items(&rig.context_of(agent), None, NonZeroUsize::new(50).unwrap())
        .await
        .unwrap();
    batch
        .items
        .into_iter()
        .filter_map(|item| match item {
            InboxItem::Mail(mail) => Some(mail.subject().to_owned()),
            InboxItem::Entry(_) => None,
        })
        .collect()
}

fn send_arguments(to: &[String], subject: &str) -> String {
    let recipients: Vec<String> = to.iter().map(|agent| format!("\"{agent}\"")).collect();
    format!(
        "{{\"to\":[{}],\"subject\":{subject:?},\"body\":\"hello\"}}",
        recipients.join(",")
    )
}

#[tokio::test(start_paused = true)]
async fn given_the_send_mail_tool_when_the_model_calls_it_then_the_mail_reaches_the_recipient_from_the_agent()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    let other = rig.agent("critic").await;
    rig.start_as(agent, "go").await;
    let arguments = send_arguments(&[other.as_uuid().to_string()], "need help");
    let model = Scripted::new(vec![
        Reply::call_with("send_mail", &arguments),
        Reply::text("sent"),
    ]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    assert_eq!(mails_for(&rig, other).await, vec!["need help"]);
    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(!stored[0].0, "{}", stored[0].1);
    assert!(stored[0].1.contains("was sent"));
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_the_inbox_refuses_when_the_model_sends_it_then_the_result_is_an_error_with_the_reason()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let arguments = send_arguments(&[], "nobody");
    let model = Scripted::new(vec![
        Reply::call_with("send_mail", &arguments),
        Reply::text("sent"),
    ]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(stored[0].0);
    assert!(
        stored[0].1.contains("at least one recipient"),
        "{}",
        stored[0].1
    );
}

#[tokio::test(start_paused = true)]
async fn given_arguments_that_are_not_valid_when_the_model_sends_a_mail_then_the_result_is_an_error_and_the_loop_goes_on()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    rig.start_as(agent, "go").await;
    let model = Scripted::new(vec![
        Reply::call_with(
            "send_mail",
            "{\"to\":[\"nobody\"],\"subject\":\"s\",\"body\":\"b\"}",
        ),
        Reply::text("done"),
    ]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(stored[0].0);
    assert!(
        stored[0].1.contains("not an agent identity"),
        "{}",
        stored[0].1
    );
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_in_the_thread_when_the_model_gets_its_files_then_it_receives_the_documents() {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    let other = rig.agent("critic").await;
    rig.start_as(agent, "go").await;
    rig.hub
        .put_document(DocumentRef::new("doc-1"), b"file content".to_vec());
    let mail = rig
        .send_mail(
            other,
            agent,
            "with a file",
            vec![Attachment::new("notes.txt", DocumentRef::new("doc-1"))],
        )
        .await;
    let arguments = format!("{{\"mail_id\":\"{mail}\"}}");
    let model = Scripted::new(vec![
        Reply::call_with("get_mail_files", &arguments),
        Reply::text("read"),
    ]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(!stored[0].0, "{}", stored[0].1);
    assert!(stored[0].1.contains("file content"));
    assert!(stored[0].1.contains("notes.txt"));
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_the_thread_does_not_hold_when_the_model_gets_its_files_then_the_result_is_an_error()
 {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    let other = rig.agent("critic").await;
    let third = rig.agent("generator").await;
    rig.start_as(agent, "go").await;
    rig.hub
        .put_document(DocumentRef::new("doc-2"), b"secret".to_vec());
    let foreign = rig
        .send_mail(
            other,
            third,
            "not for the agent",
            vec![Attachment::new("secret.txt", DocumentRef::new("doc-2"))],
        )
        .await;
    let arguments = format!("{{\"mail_id\":\"{foreign}\"}}");
    let model = Scripted::new(vec![
        Reply::call_with("get_mail_files", &arguments),
        Reply::text("tried"),
    ]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(stored[0].0);
    assert!(!stored[0].1.contains("secret"), "{}", stored[0].1);
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_without_documents_when_the_model_gets_its_files_then_the_result_says_so() {
    let rig = Rig::new().with_inbox();
    let agent = rig.agent("agent").await;
    let other = rig.agent("critic").await;
    rig.start_as(agent, "go").await;
    let mail = rig.send_mail(other, agent, "plain", Vec::new()).await;
    let arguments = format!("{{\"mail_id\":\"{mail}\"}}");
    let model = Scripted::new(vec![
        Reply::call_with("get_mail_files", &arguments),
        Reply::text("read"),
    ]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = conversation(&rig.hub, &rig.thread);
    let stored = texts_of_results(ai_turns(&conversation)[0]);
    assert!(!stored[0].0);
    assert!(stored[0].1.contains("no document"), "{}", stored[0].1);
}
