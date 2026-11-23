use br_agentic_contract::{EntryItem, InvalidEntryItem};
use br_llm_messages::{AgentMail, IncomingEntry};

use crate::common::*;

#[test]
fn given_a_human_message_a_notification_and_a_notice_when_an_entry_item_is_made_then_each_is_accepted_and_keeps_its_id()
 {
    for item in [human("hi"), notification(), notice("fyi")] {
        let entry = item.entry().clone();

        let made = EntryItem::new(entry.clone()).unwrap();

        assert_eq!(made.id(), entry.id());
        assert_eq!(made.into_entry(), entry);
    }
}

#[test]
fn given_an_agent_mail_when_an_entry_item_is_made_then_it_is_refused_because_a_mail_is_a_mail_item()
{
    let mail = AgentMail::new(
        entry_id(),
        agent_id(),
        author("sender"),
        conversation_id(),
        vec![text_block("hello")],
    )
    .unwrap();

    assert_eq!(
        EntryItem::new(IncomingEntry::from(mail)),
        Err(InvalidEntryItem)
    );
}
