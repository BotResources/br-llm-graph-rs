use std::time::Duration;

use br_agentic_contract::{DocumentRef, InboxContext, MailId, ToolSet};
use br_agentic_nodes::{GET_MAIL_FILES, SEND_MAIL, Slot, StepLimit, Timeouts, Worker, WorkerParts};
use br_graph_contract::CancelToken;
use br_llm_messages::{
    AgentMail, Conversation, ConversationId, IncomingEntry, InputTurn, Turn, TurnId,
};

use crate::common::*;
use crate::kit::*;

fn parts<'a>(rig: &'a Rig, cancel: &CancelToken, with_inbox: bool) -> WorkerParts<'a> {
    WorkerParts {
        access: &*rig.access,
        inbox: if with_inbox {
            Some(rig.inbox.clone())
        } else {
            None
        },
        context: InboxContext {
            instance: instance_id(),
            thread: rig.thread,
            agent: rig.agent,
        },
        step_limit: StepLimit::new(5).unwrap(),
        timeouts: Timeouts::new(Duration::from_secs(5), Duration::from_secs(5)).unwrap(),
        cancel: cancel.view(),
    }
}

fn rig_holding_a_mail() -> (Rig, MailId) {
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
    let mut conversation = Conversation::new(id);
    let turn = InputTurn::new(turn_id(), vec![IncomingEntry::from(agent_mail)]).unwrap();
    conversation.push_turn(Turn::from(turn)).unwrap();
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
    (rig, written)
}

#[tokio::test(start_paused = true)]
async fn given_a_worker_with_an_inbox_when_it_gives_its_mail_tools_then_they_serve_its_own_conversation_only()
 {
    let (rig, written) = rig_holding_a_mail();
    let cancel = CancelToken::new();
    let worker = Worker::open(parts(&rig, &cancel, true))
        .await
        .unwrap()
        .with_author(author("Builder"));
    let mail = worker.mail_tools().unwrap();
    let mut worker = worker.with_tools(ToolSet::new(Vec::new(), mail).unwrap());
    let model = Scripted::new(vec![Reply::Calls(vec![
        (
            GET_MAIL_FILES.to_owned(),
            format!("{{\"mail_id\":\"{written}\"}}"),
        ),
        (
            GET_MAIL_FILES.to_owned(),
            format!("{{\"mail_id\":\"{}\"}}", mail_id()),
        ),
    ])]);
    let mut slot = Slot::new(list(&[&model]));
    let mut turn: Option<TurnId> = None;

    let response = worker.step(&mut slot, &mut turn, None).await.unwrap();
    worker.tool_round(turn.unwrap(), &response).await.unwrap();

    let offered: Vec<String> = model.requests()[0]
        .tools()
        .iter()
        .map(|spec| spec.name().as_str().to_owned())
        .collect();
    assert_eq!(offered.len(), 2, "{offered:?}");
    assert!(offered.contains(&SEND_MAIL.to_owned()), "{offered:?}");
    assert!(offered.contains(&GET_MAIL_FILES.to_owned()), "{offered:?}");
    let results = outcomes(&rig.conversation());
    assert_eq!(results.len(), 2);
    let served: Vec<&Outcome> = results.iter().filter(|r| !r.is_error).collect();
    assert_eq!(served.len(), 1);
    assert!(served[0].text().contains("first.txt"));
    assert!(served[0].text().contains("first start content"));
    assert_eq!(results.iter().filter(|r| r.is_error).count(), 1);
}

#[tokio::test(start_paused = true)]
async fn given_a_worker_without_an_inbox_when_it_gives_its_mail_tools_then_the_list_is_empty() {
    let rig = Rig::new();
    let cancel = CancelToken::new();
    let worker = Worker::open(parts(&rig, &cancel, false))
        .await
        .unwrap()
        .with_author(author("Builder"));

    let mail = worker.mail_tools().unwrap();

    assert!(mail.is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_worker_with_an_inbox_and_no_author_when_it_gives_its_mail_tools_then_it_is_a_fault()
 {
    let rig = Rig::new();
    let cancel = CancelToken::new();
    let worker = Worker::open(parts(&rig, &cancel, true)).await.unwrap();

    let mail = worker.mail_tools();

    assert!(mail.is_err());
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_in_the_inbox_when_the_worker_drains_at_start_then_the_mail_is_stored_as_an_input_turn_and_marked_read()
 {
    let rig = Rig::new();
    let mail = new_mail(&[rig.agent], &[], "at start", "hello");
    let id = mail.id;
    rig.inbox.deliver_mail(agent_id(), mail).unwrap();
    let cancel = CancelToken::new();
    let mut worker = Worker::open(parts(&rig, &cancel, true))
        .await
        .unwrap()
        .with_author(author("Builder"));

    let drained = worker.drain_at_start().await.unwrap();

    assert_eq!(drained.stored, 1);
    assert!(drained.request_stored);
    let kept = rig.conversation();
    let stored = incoming_with(&kept, id.entry_id()).expect("the mail is stored");
    assert!(matches!(stored, IncomingEntry::AgentMail(_)));
    assert_eq!(count_of(&kept, id.entry_id()), 1);
    assert!(rig.inbox.is_read(rig.agent, id.entry_id()));
}

#[tokio::test(start_paused = true)]
async fn given_a_worker_without_an_inbox_when_it_drains_at_start_then_nothing_is_stored() {
    let rig = Rig::new();
    let cancel = CancelToken::new();
    let mut worker = Worker::open(parts(&rig, &cancel, false))
        .await
        .unwrap()
        .with_author(author("Builder"));

    let drained = worker.drain_at_start().await.unwrap();

    assert_eq!(drained, br_agentic_nodes::Drained::default());
    assert_eq!(rig.conversation().turns().len(), 1);
}
