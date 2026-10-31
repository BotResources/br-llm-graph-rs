use br_llm_messages::{
    Conversation, EntryId, IncomingEntry, QueuePosition, RequestAccepted, RequestId, RuntimeNotice,
    TaskCompleted, TaskFailed, TaskId, TaskStarted,
};

use crate::common::*;
use crate::kit::*;

async fn run(rig: &Rig) -> Conversation {
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);
    let update = Driver::result(&node).await;
    assert_eq!(update.steps, Some(1));
    rig.conversation()
}

fn delivered_turn(conversation: &Conversation) -> Vec<IncomingEntry> {
    let turns = input_turns(conversation);
    assert_eq!(turns.len(), 2, "the start turn and the turn of the drain");
    turns[1].entries().to_vec()
}

fn notice(text: &str) -> IncomingEntry {
    IncomingEntry::from(RuntimeNotice::new(entry_id(), text).unwrap())
}

fn notifications() -> Vec<IncomingEntry> {
    let request = RequestId::from_uuid(uuid()).unwrap();
    let task = TaskId::from_uuid(uuid()).unwrap();
    vec![
        IncomingEntry::from(RequestAccepted::new(entry_id(), request)),
        IncomingEntry::from(QueuePosition::new(entry_id(), request, 2).unwrap()),
        IncomingEntry::from(TaskStarted::new(entry_id(), request, task)),
        IncomingEntry::from(TaskCompleted::new(entry_id(), task)),
        IncomingEntry::from(TaskFailed::new(entry_id(), task, "it broke").unwrap()),
    ]
}

#[tokio::test(start_paused = true)]
async fn given_a_human_message_in_the_inbox_when_the_agent_runs_then_it_is_stored_whole() {
    let rig = Rig::new().with_inbox();
    let (entry, _) = human("please look at this");
    rig.inbox.deliver_entry(rig.agent, entry.clone());

    let conversation = run(&rig).await;

    assert_eq!(delivered_turn(&conversation), vec![entry.into_entry()]);
}

#[tokio::test(start_paused = true)]
async fn given_runtime_notifications_in_the_inbox_when_the_agent_runs_then_each_is_stored_whole() {
    let rig = Rig::new().with_inbox();
    let entries = notifications();
    for entry in &entries {
        rig.inbox.deliver_entry(
            rig.agent,
            br_agentic_contract::InboxEntry::new(entry.clone()).unwrap(),
        );
    }

    let conversation = run(&rig).await;

    assert_eq!(delivered_turn(&conversation), entries);
}

#[tokio::test(start_paused = true)]
async fn given_a_runtime_notice_in_the_inbox_when_the_agent_runs_then_it_is_stored_whole() {
    let rig = Rig::new().with_inbox();
    let entry = notice("the deploy window opened");
    rig.inbox.deliver_entry(
        rig.agent,
        br_agentic_contract::InboxEntry::new(entry.clone()).unwrap(),
    );

    let conversation = run(&rig).await;

    assert_eq!(delivered_turn(&conversation), vec![entry]);
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_to_the_agent_when_the_agent_runs_then_an_agent_mail_with_the_mail_id_is_stored()
 {
    let rig = Rig::new().with_inbox();
    let sender = agent_id();
    let mail = new_mail(&[rig.agent], &[], "subject", "body");
    let id = mail.id;
    rig.inbox.deliver_mail(sender, mail).unwrap();

    let conversation = run(&rig).await;

    let stored = delivered_turn(&conversation);
    assert_eq!(stored.len(), 1);
    assert!(matches!(&stored[0], IncomingEntry::AgentMail(_)));
    assert_eq!(stored[0].id(), id.entry_id());
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_in_copy_when_the_agent_runs_then_a_runtime_notice_with_the_mail_id_is_stored()
{
    let rig = Rig::new().with_inbox();
    let sender = agent_id();
    let mail = new_mail(&[agent_id()], &[rig.agent], "subject", "body");
    let id = mail.id;
    rig.inbox.deliver_mail(sender, mail).unwrap();

    let conversation = run(&rig).await;

    let stored = delivered_turn(&conversation);
    assert_eq!(stored.len(), 1);
    assert!(matches!(&stored[0], IncomingEntry::RuntimeNotice(_)));
    assert_eq!(stored[0].id(), id.entry_id());
}

#[tokio::test(start_paused = true)]
async fn given_items_of_every_kind_when_the_agent_runs_then_all_are_stored_in_order_of_arrival() {
    let rig = Rig::new().with_inbox();
    let sender = agent_id();
    let (first, first_id) = human("first");
    let mail_to = new_mail(&[rig.agent], &[], "to subject", "to body");
    let mail_cc = new_mail(&[agent_id()], &[rig.agent], "cc subject", "cc body");
    let (last, last_id) = human("last");
    let mut expected: Vec<EntryId> = vec![first_id, mail_to.id.entry_id(), mail_cc.id.entry_id()];
    rig.inbox.deliver_entry(rig.agent, first);
    rig.inbox.deliver_mail(sender, mail_to).unwrap();
    rig.inbox.deliver_mail(sender, mail_cc).unwrap();
    let notification = notifications().remove(0);
    expected.push(notification.id());
    rig.inbox.deliver_entry(
        rig.agent,
        br_agentic_contract::InboxEntry::new(notification).unwrap(),
    );
    rig.inbox.deliver_entry(rig.agent, last);
    expected.push(last_id);

    let conversation = run(&rig).await;

    let stored: Vec<EntryId> = delivered_turn(&conversation)
        .iter()
        .map(IncomingEntry::id)
        .collect();
    assert_eq!(stored, expected);
}
