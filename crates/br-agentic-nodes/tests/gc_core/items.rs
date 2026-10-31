use br_llm_messages::{EntryId, IncomingEntry};

use crate::common::*;

pub fn human_item(rig: &Rig, text: &str) -> (EntryId, impl Fn() + Send + Sync + 'static) {
    let message = human_message(text);
    let id = message.id();
    let entry = br_agentic_contract::InboxEntry::new(IncomingEntry::from(message)).unwrap();
    let inbox = rig.inbox.clone();
    let agent = rig.generator;
    (id, move || inbox.deliver_entry(agent, entry.clone()))
}

pub fn notice_item(rig: &Rig, text: &str) -> (EntryId, impl Fn() + Send + Sync + 'static) {
    let notice = br_llm_messages::RuntimeNotice::new(entry_id(), text).unwrap();
    let id = notice.id();
    let entry = br_agentic_contract::InboxEntry::new(IncomingEntry::RuntimeNotice(notice)).unwrap();
    let inbox = rig.inbox.clone();
    let agent = rig.generator;
    (id, move || inbox.deliver_entry(agent, entry.clone()))
}

pub fn mail_item(rig: &Rig, copy: bool) -> (EntryId, impl Fn() + Send + Sync + 'static) {
    let id = mail_id();
    let recipient = rig.generator;
    let other = agent_id();
    let (to, cc) = if copy {
        (vec![other], vec![recipient])
    } else {
        (vec![recipient], Vec::new())
    };
    let mail = br_agentic_contract::NewMail {
        id,
        author: author("Sender"),
        source: br_llm_messages::ConversationId::from_uuid(uuid()).unwrap(),
        to,
        cc,
        subject: "subject".to_owned(),
        body: "body".to_owned(),
        attachments: Vec::new(),
    };
    let inbox = rig.inbox.clone();
    let sender = agent_id();
    (id.entry_id(), move || {
        inbox.deliver_mail(sender, mail.clone()).unwrap();
    })
}
