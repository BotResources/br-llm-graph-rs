use br_agentic_contract::InboxEntry;
use br_llm_messages::{
    AgentMail, Conversation, ConversationId, EntryId, HumanMessage, IncomingEntry, InputTurn, Turn,
};

use crate::common::*;
use crate::kit::*;
use crate::spy::{Items, Spy};

fn holding(id: ConversationId, entry: IncomingEntry) -> Conversation {
    let mut conversation = Conversation::new(id);
    let turn = InputTurn::new(turn_id(), vec![entry]).unwrap();
    conversation.push_turn(Turn::from(turn)).unwrap();
    conversation
}

fn rig_holding(entry: IncomingEntry) -> Rig {
    let id = ConversationId::from_uuid(uuid()).unwrap();
    Rig::with_conversation(id, holding(id, entry))
}

fn held_message(text: &str) -> (HumanMessage, EntryId) {
    let message = HumanMessage::new(entry_id(), human_id(), vec![text_block(text)]).unwrap();
    let id = message.id();
    (message, id)
}

fn no_turn_added(rig: &Rig) -> bool {
    !names(&rig.record).contains(&"add_input_turn")
}

#[tokio::test(start_paused = true)]
async fn given_an_item_the_conversation_holds_when_the_agent_starts_then_it_is_skipped_and_marked()
{
    let (message, id) = held_message("already in the thread");
    let rig = rig_holding(IncomingEntry::from(message.clone())).with_inbox();
    rig.inbox.deliver_entry(
        rig.agent,
        InboxEntry::new(IncomingEntry::from(message)).unwrap(),
    );
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = rig.conversation();
    assert_eq!(count_of(&conversation, id), 1);
    assert_eq!(input_turns(&conversation).len(), 1);
    assert!(no_turn_added(&rig));
    assert!(rig.inbox.is_read(rig.agent, id));
}

#[tokio::test(start_paused = true)]
async fn given_a_mail_entry_written_at_first_start_when_the_agent_starts_then_the_mail_item_is_skipped_and_marked()
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
    let rig = rig_holding(IncomingEntry::from(agent_mail)).with_inbox();
    let mut mail = new_mail(&[rig.agent], &[], "first start", "delivered by the inbox");
    mail.id = written;
    mail.source = source;
    rig.inbox.deliver_mail(sender, mail).unwrap();
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let conversation = rig.conversation();
    assert_eq!(count_of(&conversation, written.entry_id()), 1);
    assert!(no_turn_added(&rig));
    assert!(rig.inbox.is_read(rig.agent, written.entry_id()));
    let sent = history_text(&model.requests()[0]);
    assert_eq!(sent.matches("written by the runner").count(), 1);
    assert_eq!(sent.matches("delivered by the inbox").count(), 0);
}

#[tokio::test(start_paused = true)]
async fn given_an_item_given_twice_in_the_queue_when_the_agent_starts_then_it_is_stored_once() {
    let rig = Rig::new();
    let spy = Spy::new(&rig);
    let (a, a_id) = human("given-twice");
    let (b, b_id) = human("given-once");
    spy.plan_items(vec![
        Items::Give(batch(vec![item(&a), item(&b)], Some("next"))),
        Items::Give(batch(vec![item(&a)], None)),
    ]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let conversation = rig.conversation();
    assert_eq!(count_of(&conversation, a_id), 1);
    assert_eq!(count_of(&conversation, b_id), 1);
    let turns = input_turns(&conversation);
    let stored: Vec<_> = turns[1].entries().iter().map(IncomingEntry::id).collect();
    assert_eq!(stored, vec![a_id, b_id]);
}

#[tokio::test(start_paused = true)]
async fn given_a_stored_item_given_again_by_a_later_read_when_a_boundary_comes_then_it_is_skipped_and_marked()
 {
    let (noop, _) = echo("noop");
    let rig = Rig::new().with_tools(vec![noop]);
    let spy = Spy::new(&rig);
    let (a, a_id) = human("given-again-later");
    spy.plan_items(vec![
        Items::Give(batch(vec![item(&a)], None)),
        Items::Give(batch(vec![item(&a)], None)),
    ]);
    let model = Scripted::new(vec![
        Reply::call("noop"),
        Reply::call("noop"),
        Reply::text("done"),
    ]);
    let node = spied(&rig, &spy, &model);

    Driver::result(&node).await;

    let conversation = rig.conversation();
    assert_eq!(count_of(&conversation, a_id), 1);
    assert_eq!(spy.times_marked(a_id), 2);
    assert_eq!(ai_turns(&conversation).len(), 1);
}
