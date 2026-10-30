use std::sync::atomic::Ordering;

use br_agentic_contract::memory::Call;
use br_agentic_contract::{
    InboxItem, InboxPort, Queue, Session, SessionError, ThreadAccess, store_at_boundary,
};
use br_llm_messages::{
    ClosureReason, EntryId, IncomingEntry, InputTurn, Turn, TurnClosure, TurnEntry,
};

use crate::common::*;
use crate::flaky::Flaky;

async fn run(
    session: &mut Session<'_>,
    world: &World,
    queue: &mut Queue,
) -> Result<br_agentic_contract::Boundary, br_agentic_contract::BoundaryError> {
    store_at_boundary(
        session,
        &world.inbox,
        &world.context(),
        queue,
        &mail_entry,
        turn_id(),
    )
    .await
}

fn queue_of(items: &[InboxItem]) -> Queue {
    let mut queue = Queue::new();
    for item in items {
        queue.push(item.clone());
    }
    queue
}

fn human_item(text: &str, world: &World) -> InboxItem {
    let entry = human(text);
    world.inbox.deliver_entry(world.agent, entry.clone());
    InboxItem::Entry(entry)
}

async fn start_with_trigger(world: &World) {
    world
        .access
        .add_input_turn(
            &world.thread,
            InputTurn::new(turn_id(), vec![human("trigger").into_entry()]).unwrap(),
        )
        .await
        .unwrap();
}

fn marks(world: &World) -> Vec<EntryId> {
    world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            Call::InboxMarkRead { item, .. } => Some(*item),
            _ => None,
        })
        .collect()
}

fn position(world: &World, wanted: impl Fn(&Call) -> bool) -> usize {
    world.record.calls().iter().position(wanted).unwrap()
}

#[test]
fn given_an_item_whose_id_is_held_earlier_in_the_queue_when_it_is_pushed_then_it_is_skipped() {
    let world = World::new();
    let item = human_item("once", &world);
    let mut queue = Queue::new();

    assert!(queue.push(item.clone()));
    assert!(!queue.push(item));
    assert_eq!(queue.len(), 1);
}

#[tokio::test]
async fn given_the_inbox_when_the_queue_reads_it_then_it_holds_every_unread_item_of_all_batches_in_order()
 {
    let world = World::new();
    let items: Vec<InboxItem> = (0..5)
        .map(|n| human_item(&format!("m{n}"), &world))
        .collect();
    let mut queue = Queue::new();

    let added = queue
        .read(&world.inbox, &world.context(), limit(2))
        .await
        .unwrap();

    assert_eq!(added, 5);
    assert_eq!(queue.items(), items.as_slice());
    let again = queue
        .read(&world.inbox, &world.context(), limit(2))
        .await
        .unwrap();
    assert_eq!(again, 0);
}

#[tokio::test]
async fn given_no_open_ai_turn_when_the_boundary_stores_items_then_they_form_one_new_input_turn_with_one_entry_for_each_item_in_arrival_order()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let items = [
        human_item("first", &world),
        human_item("second", &world),
        human_item("third", &world),
    ];
    let mut session = world.session().await;
    let turns_before = session.conversation().turns().len();
    let mut queue = queue_of(&items);

    let report = run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    assert_eq!(conversation.turns().len(), turns_before + 1);
    let Some(Turn::InputTurn(input)) = conversation.turns().last() else {
        panic!("a new input turn")
    };
    let ids: Vec<EntryId> = input.entries().iter().map(IncomingEntry::id).collect();
    assert_eq!(ids, items.iter().map(InboxItem::id).collect::<Vec<_>>());
    assert_eq!(report.stored, ids);
    assert!(queue.is_empty());
}

#[tokio::test]
async fn given_a_thread_ending_with_an_input_turn_when_the_boundary_stores_an_item_then_that_input_turn_is_not_extended()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let before = world.access.conversation();
    let item = human_item("late", &world);
    let mut session = world.session().await;
    let mut queue = queue_of(&[item]);

    run(&mut session, &world, &mut queue).await.unwrap();

    let after = world.access.conversation();
    assert_eq!(after.turns()[0], before.turns()[0]);
    assert_eq!(after.turns().len(), 2);
}

#[tokio::test]
async fn given_an_open_ai_turn_of_the_agent_when_the_boundary_stores_items_then_each_is_added_to_it_after_its_last_entry_in_arrival_order()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (turn, _call) = world.open_turn_with_call(&mut session).await;
    let items = [human_item("one", &world), human_item("two", &world)];
    let mut queue = queue_of(&items);

    run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    let ai = conversation.turn(&turn.id).unwrap().as_ai().unwrap();
    let tail: Vec<EntryId> = ai
        .entries()
        .iter()
        .rev()
        .take(2)
        .rev()
        .map(TurnEntry::id)
        .collect();
    assert_eq!(tail, items.iter().map(InboxItem::id).collect::<Vec<_>>());
    assert_eq!(conversation.turns().len(), 1);
}

#[tokio::test]
async fn given_a_closed_ai_turn_of_the_agent_when_the_boundary_stores_an_item_then_a_new_input_turn_holds_it()
 {
    let world = World::new();
    let mut session = world.session().await;
    let turn = world.ai(turn_id());
    session
        .add_entry(&turn, TurnEntry::from(response_text(entry_id(), "done")))
        .await
        .unwrap();
    session
        .close_turn(
            &turn,
            TurnClosure::new(entry_id(), ClosureReason::completed()),
        )
        .await
        .unwrap();
    let item = human_item("after", &world);
    let mut queue = queue_of(&[item]);

    run(&mut session, &world, &mut queue).await.unwrap();

    assert!(matches!(
        world.access.conversation().turns().last(),
        Some(Turn::InputTurn(_))
    ));
}

#[tokio::test]
async fn given_an_open_ai_turn_of_another_agent_when_the_boundary_stores_an_item_then_a_new_input_turn_holds_it_and_the_other_turn_is_unchanged()
 {
    let world = World::new();
    let mut session = world.session().await;
    let mut other = world.ai(turn_id());
    other.agent = agent_id();
    other.author = author("critic");
    session
        .add_entry(&other, TurnEntry::from(response_text(entry_id(), "theirs")))
        .await
        .unwrap();
    let before = world.access.conversation().turns()[0].clone();
    let item = human_item("mine", &world);
    let mut queue = queue_of(&[item]);

    run(&mut session, &world, &mut queue).await.unwrap();

    let after = world.access.conversation();
    assert_eq!(after.turns()[0], before);
    assert!(matches!(after.turns().last(), Some(Turn::InputTurn(_))));
}

#[tokio::test]
async fn given_an_item_whose_entry_the_conversation_holds_when_the_boundary_runs_then_it_is_skipped_and_marked_as_read_and_not_stored_again()
 {
    let world = World::new();
    let item = human_item("held", &world);
    let InboxItem::Entry(entry) = &item else {
        panic!()
    };
    world
        .access
        .add_input_turn(
            &world.thread,
            InputTurn::new(turn_id(), vec![entry.entry().clone()]).unwrap(),
        )
        .await
        .unwrap();
    let mut session = world.session().await;
    let turns = session.conversation().turns().len();
    let mut queue = queue_of(std::slice::from_ref(&item));

    let report = run(&mut session, &world, &mut queue).await.unwrap();

    assert_eq!(report.skipped, [item.id()]);
    assert!(report.stored.is_empty());
    assert_eq!(world.access.conversation().turns().len(), turns);
    assert!(world.inbox.is_read(world.agent, item.id()));
    assert!(queue.is_empty());
}

#[tokio::test]
async fn given_a_mail_item_when_the_boundary_stores_it_then_the_entry_id_is_the_mail_id_and_the_entry_holds_the_sender_the_author_name_and_the_source_conversation_of_the_item()
 {
    let world = World::new();
    let sender = agent_id();
    let mail = new_mail(&[world.agent], &[], "hello");
    let (id, name, source) = (mail.id, mail.author.clone(), mail.source);
    world.inbox.deliver_mail(sender, mail).unwrap();
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);

    run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    let Turn::InputTurn(input) = &conversation.turns()[0] else {
        panic!()
    };
    let IncomingEntry::AgentMail(entry) = &input.entries()[0] else {
        panic!("an agent mail")
    };
    assert_eq!(entry.id(), id.entry_id());
    assert_eq!(entry.sender(), sender);
    assert_eq!(entry.author(), &name);
    assert_eq!(entry.source_conversation(), source);
}

#[tokio::test]
async fn given_a_cc_mail_item_when_the_boundary_stores_it_then_the_entry_is_a_runtime_notice_with_the_mail_id()
 {
    let world = World::new();
    let mail = new_mail(&[agent_id()], &[world.agent], "fyi");
    let id = mail.id;
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);

    run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    let Turn::InputTurn(input) = &conversation.turns()[0] else {
        panic!()
    };
    assert!(
        matches!(&input.entries()[0], IncomingEntry::RuntimeNotice(n) if n.id() == id.entry_id())
    );
}

#[tokio::test]
async fn given_a_human_message_a_notification_and_a_notice_when_the_boundary_stores_them_then_each_entry_is_the_whole_entry_as_the_inbox_gave_it()
 {
    let world = World::new();
    let given = [human("h"), notification(), notice("n")];
    for entry in &given {
        world.inbox.deliver_entry(world.agent, entry.clone());
    }
    let items: Vec<InboxItem> = given.iter().cloned().map(InboxItem::Entry).collect();
    let mut session = world.session().await;
    let mut queue = queue_of(&items);

    run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    let Turn::InputTurn(input) = &conversation.turns()[0] else {
        panic!()
    };
    let stored: Vec<IncomingEntry> = input.entries().to_vec();
    let expected: Vec<IncomingEntry> = given.iter().map(|entry| entry.entry().clone()).collect();
    assert_eq!(stored, expected);
}

#[tokio::test]
async fn given_stored_items_when_the_boundary_marks_them_then_every_kind_of_item_is_marked_and_only_after_the_thread_access_has_been_handed_the_entry()
 {
    let world = World::new();
    let mail = new_mail(&[world.agent], &[], "hello");
    let mail_item_id = mail.id.entry_id();
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    let items_ids = [human_item("h", &world).id(), mail_item_id];
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);

    run(&mut session, &world, &mut queue).await.unwrap();

    let marked = marks(&world);
    for id in items_ids {
        assert!(marked.contains(&id));
        assert!(world.inbox.is_read(world.agent, id));
    }
    let handed = position(&world, |call| matches!(call, Call::AddInputTurn { .. }));
    let first_mark = position(&world, |call| matches!(call, Call::InboxMarkRead { .. }));
    assert!(handed < first_mark);
}

#[tokio::test]
async fn given_an_open_ai_turn_when_the_boundary_stores_items_then_each_item_is_marked_after_its_own_entry_is_handed_over()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (_turn, _call) = world.open_turn_with_call(&mut session).await;
    let items = [human_item("one", &world), human_item("two", &world)];
    let mut queue = queue_of(&items);

    run(&mut session, &world, &mut queue).await.unwrap();

    let calls = world.record.calls();
    let kinds: Vec<&str> = calls
        .iter()
        .filter_map(|call| match call {
            Call::AddEntry { entry, .. }
                if entry.id() == items[0].id() || entry.id() == items[1].id() =>
            {
                Some("add")
            }
            Call::InboxMarkRead { .. } => Some("mark"),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, ["add", "mark", "add", "mark"]);
}

#[tokio::test]
async fn given_a_thread_access_that_fails_when_the_boundary_stores_items_then_nothing_is_marked_the_items_stay_queued_and_the_fault_is_reported_without_a_retry()
 {
    let world = World::new();
    let items = [human_item("a", &world), human_item("b", &world)];
    let flaky = Flaky::new(world.access.clone());
    let mut session = Session::start(&flaky, world.thread).await.unwrap();
    flaky.fail_adds.store(true, Ordering::SeqCst);
    let mut queue = queue_of(&items);

    let error = run(&mut session, &world, &mut queue).await.unwrap_err();

    assert!(matches!(error.fault, SessionError::Port(_)));
    assert!(marks(&world).is_empty());
    assert_eq!(queue.items(), items.as_slice());
    assert_eq!(flaky.failed_calls.load(Ordering::SeqCst), 1);
    assert!(!world.inbox.is_read(world.agent, items[0].id()));
}

#[tokio::test]
async fn given_a_failing_second_store_in_an_open_turn_when_the_boundary_runs_then_the_first_item_is_marked_and_the_second_stays_queued_and_unread()
 {
    let world = World::new();
    let flaky = Flaky::new(world.access.clone());
    let mut session = Session::start(&flaky, world.thread).await.unwrap();
    let (_turn, _call) = world.open_turn_with_call(&mut session).await;
    let items = [human_item("ok", &world), human_item("fails", &world)];
    flaky.fail_adds.store(true, Ordering::SeqCst);
    flaky.ok_adds.store(1, Ordering::SeqCst);
    let mut queue = queue_of(&items);

    let error = run(&mut session, &world, &mut queue).await.unwrap_err();

    assert_eq!(error.done.stored, [items[0].id()]);
    assert!(world.inbox.is_read(world.agent, items[0].id()));
    assert!(!world.inbox.is_read(world.agent, items[1].id()));
    assert_eq!(queue.items(), &items[1..]);
}

#[tokio::test]
async fn given_a_mail_entry_that_cannot_be_built_when_the_boundary_runs_then_nothing_is_stored_or_marked_and_the_items_stay_queued()
 {
    let world = World::new();
    world
        .inbox
        .deliver_mail(agent_id(), new_mail(&[world.agent], &[], "hello"))
        .unwrap();
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);
    let failing =
        |_: &br_agentic_contract::MailItem| Err(br_llm_messages::PlainText::new(" ").unwrap_err());

    let error = store_at_boundary(
        &mut session,
        &world.inbox,
        &world.context(),
        &mut queue,
        &failing,
        turn_id(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error.fault, SessionError::Rules(_)));
    assert!(marks(&world).is_empty());
    assert_eq!(queue.len(), 1);
    assert!(world.access.conversation().turns().is_empty());
}

#[tokio::test]
async fn given_a_queue_with_items_when_the_instance_ends_without_a_boundary_then_the_items_are_not_marked_and_stay_in_the_inbox()
 {
    let world = World::new();
    let item = human_item("never stored", &world);
    let mut queue = Queue::new();
    queue
        .read(&world.inbox, &world.context(), limit(5))
        .await
        .unwrap();

    drop(queue);

    assert!(marks(&world).is_empty());
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    assert_eq!(batch.items[0].id(), item.id());
}

#[tokio::test]
async fn given_a_mail_with_attachments_when_the_boundary_stores_it_then_no_document_is_read() {
    let world = World::new();
    let mut mail = new_mail(&[world.agent], &[], "files");
    mail.attachments = vec![br_agentic_contract::Attachment::new(
        "a.txt",
        br_agentic_contract::DocumentRef::new("ref"),
    )];
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);

    run(&mut session, &world, &mut queue).await.unwrap();

    assert!(
        !world
            .record
            .calls()
            .iter()
            .any(|call| matches!(call, Call::InboxDocuments { .. }))
    );
}

#[tokio::test]
async fn given_an_empty_queue_when_the_boundary_runs_then_nothing_is_added_to_the_thread() {
    let world = World::new();
    let mut session = world.session().await;
    let mut queue = Queue::new();

    let report = run(&mut session, &world, &mut queue).await.unwrap();

    assert!(report.stored.is_empty() && report.skipped.is_empty());
    assert!(world.access.conversation().turns().is_empty());
}

#[tokio::test]
async fn given_a_boundary_that_stored_items_when_the_conversation_is_read_back_then_the_session_copy_equals_what_the_thread_access_holds()
 {
    let world = World::new();
    let item = human_item("sync", &world);
    let mut session = world.session().await;
    let mut queue = queue_of(&[item]);

    run(&mut session, &world, &mut queue).await.unwrap();

    assert_eq!(session.conversation(), &world.access.conversation());
}
