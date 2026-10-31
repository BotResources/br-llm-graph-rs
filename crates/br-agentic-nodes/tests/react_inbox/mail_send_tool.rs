use br_agentic_contract::NewMail;
use br_agentic_contract::memory::Call;
use br_agentic_nodes::SEND_MAIL;
use br_llm_clients_core::{ParameterKind, ToolSpec};
use br_llm_messages::AgentId;

use crate::common::*;
use crate::kit::*;

pub fn ids(agents: &[AgentId]) -> String {
    agents
        .iter()
        .map(|agent| format!("{:?}", agent.as_uuid().to_string()))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn send_arguments(
    to: &[AgentId],
    cc: &[AgentId],
    subject: &str,
    body: &str,
    attachments: &[(&str, &str)],
) -> String {
    let attachments = attachments
        .iter()
        .map(|(name, document)| format!("{{\"name\":{name:?},\"document\":{document:?}}}"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"to\":[{}],\"cc\":[{}],\"subject\":{subject:?},\"body\":{body:?},\"attachments\":[{attachments}]}}",
        ids(to),
        ids(cc)
    )
}

pub fn sent_mails(rig: &Rig) -> Vec<(br_agentic_contract::InboxContext, NewMail)> {
    rig.record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::InboxSend { context, mail } => Some((context, mail)),
            _ => None,
        })
        .collect()
}

pub async fn run_sends(rig: &Rig, calls: Vec<Reply>) -> Vec<Outcome> {
    let mut replies = calls;
    replies.push(Reply::text("done"));
    let model = Scripted::new(replies);
    let node = plain(rig, &model, "Agent");
    let update = Driver::result(&node).await;
    assert_eq!(update.text, "done");
    outcomes(&rig.conversation())
}

fn parameter<'a>(spec: &'a ToolSpec, name: &str) -> &'a br_llm_clients_core::Parameter {
    spec.parameters()
        .iter()
        .find(|parameter| parameter.name() == name)
        .unwrap_or_else(|| panic!("no parameter {name}"))
}

fn is_string_array(kind: &ParameterKind) -> bool {
    matches!(kind, ParameterKind::Array { items } if matches!(**items, ParameterKind::String))
}

#[tokio::test(start_paused = true)]
async fn given_a_send_mail_call_when_the_tool_runs_then_the_instance_gives_the_mail_id_the_sender_the_author_and_the_source()
 {
    let rig = Rig::new();
    let (to, cc) = (agent_id(), agent_id());
    let arguments = send_arguments(
        &[to],
        &[cc],
        "Weekly plan",
        "Here is the plan",
        &[("plan.md", "doc-1"), ("budget.csv", "doc-2")],
    );

    let results = run_sends(&rig, vec![Reply::call_with(SEND_MAIL, &arguments)]).await;

    assert!(!results[0].is_error);
    assert!(!results[0].text().trim().is_empty());
    let sent = sent_mails(&rig);
    assert_eq!(sent.len(), 1);
    let (context, mail) = &sent[0];
    assert_eq!(context.agent, rig.agent);
    assert_eq!(mail.author.as_str(), "Agent");
    assert_eq!(mail.source, rig.conversation_id);
    assert_eq!(mail.to, vec![to]);
    assert_eq!(mail.cc, vec![cc]);
    assert_eq!(mail.subject, "Weekly plan");
    assert_eq!(mail.body, "Here is the plan");
    let attachments: Vec<(&str, &str)> = mail
        .attachments
        .iter()
        .map(|a| (a.name(), a.document().as_str()))
        .collect();
    assert_eq!(
        attachments,
        vec![("plan.md", "doc-1"), ("budget.csv", "doc-2")]
    );
    let stored = rig
        .inbox
        .mail(mail.id)
        .expect("the inbox accepted the mail");
    assert_eq!(stored.sender, rig.agent);
}

#[tokio::test(start_paused = true)]
async fn given_two_send_mail_calls_when_the_tool_runs_then_each_mail_gets_its_own_id() {
    let rig = Rig::new();
    let to = agent_id();
    let arguments = send_arguments(&[to], &[], "same", "same", &[]);

    let results = run_sends(
        &rig,
        vec![
            Reply::call_with(SEND_MAIL, &arguments),
            Reply::call_with(SEND_MAIL, &arguments),
        ],
    )
    .await;

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|result| !result.is_error));
    let sent = sent_mails(&rig);
    assert_eq!(sent.len(), 2);
    assert_ne!(sent[0].1.id, sent[1].1.id);
}
