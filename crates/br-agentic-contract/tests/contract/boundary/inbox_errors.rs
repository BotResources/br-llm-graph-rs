use br_agentic_contract::mail_documents;

use super::*;
use crate::failing_inbox::{FailingInbox, FailingMarks};

async fn run_with(
    inbox: &FailingInbox<'_>,
    session: &mut Session<'_>,
    world: &World,
    queue: &mut Queue,
) -> Result<br_agentic_contract::Boundary, br_agentic_contract::BoundaryError> {
    store_at_boundary(
        session,
        inbox,
        &world.context(),
        queue,
        &mail_entry,
        turn_id(),
    )
    .await
}

fn ids(items: &[InboxItem]) -> Vec<EntryId> {
    items.iter().map(InboxItem::id).collect()
}

#[tokio::test]
async fn given_an_inbox_whose_marks_fail_when_the_boundary_stores_items_in_a_new_input_turn_then_every_item_is_stored_and_no_error_is_returned()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let items = [human_item("a", &world), human_item("b", &world)];
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.marks = FailingMarks::All;
    let mut session = world.session().await;
    let mut queue = queue_of(&items);

    let report = run_with(&inbox, &mut session, &world, &mut queue)
        .await
        .unwrap();

    assert_eq!(report.stored, ids(&items));
    assert_eq!(report.unmarked, ids(&items));
    assert_eq!(inbox.mark_calls(), items.len());
    assert!(queue.is_empty());
    assert_eq!(session.conversation(), &world.access.conversation());
    assert!(session.holds(items[0].id()) && session.holds(items[1].id()));
}

#[tokio::test]
async fn given_an_inbox_whose_marks_fail_when_the_boundary_stores_items_in_an_open_ai_turn_then_every_item_is_stored_and_no_error_is_returned()
 {
    let world = World::new();
    let mut session = world.session().await;
    let (_turn, _call) = world.open_turn_with_call(&mut session).await;
    let items = [human_item("a", &world), human_item("b", &world)];
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.marks = FailingMarks::All;
    let mut queue = queue_of(&items);

    let report = run_with(&inbox, &mut session, &world, &mut queue)
        .await
        .unwrap();

    assert_eq!(report.stored, ids(&items));
    assert_eq!(report.unmarked, ids(&items));
    assert_eq!(inbox.mark_calls(), items.len());
    assert!(queue.is_empty());
    assert!(session.holds(items[0].id()) && session.holds(items[1].id()));
}

#[tokio::test]
async fn given_an_inbox_that_fails_the_mark_of_one_item_when_the_boundary_runs_then_the_next_items_are_stored_and_marked_and_only_that_item_is_reported_unmarked()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let items = [
        human_item("a", &world),
        human_item("b", &world),
        human_item("c", &world),
    ];
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.marks = FailingMarks::Only(vec![items[1].id()]);
    let mut session = world.session().await;
    let mut queue = queue_of(&items);

    let report = run_with(&inbox, &mut session, &world, &mut queue)
        .await
        .unwrap();

    assert_eq!(report.stored, ids(&items));
    assert_eq!(report.unmarked, [items[1].id()]);
    assert!(world.inbox.is_read(world.agent, items[0].id()));
    assert!(!world.inbox.is_read(world.agent, items[1].id()));
    assert!(world.inbox.is_read(world.agent, items[2].id()));
}

#[tokio::test]
async fn given_an_inbox_whose_marks_fail_when_the_boundary_skips_items_the_conversation_holds_then_they_are_skipped_and_no_error_is_returned()
 {
    let world = World::new();
    let item = human_item("held", &world);
    let mut session = world.session().await;
    let mut first = queue_of(std::slice::from_ref(&item));
    run(&mut session, &world, &mut first).await.unwrap();
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.marks = FailingMarks::All;
    let mut queue = queue_of(std::slice::from_ref(&item));

    let report = run_with(&inbox, &mut session, &world, &mut queue)
        .await
        .unwrap();

    assert_eq!(report.skipped, [item.id()]);
    assert_eq!(report.unmarked, [item.id()]);
    assert_eq!(inbox.mark_calls(), 1);
    assert!(report.stored.is_empty());
    assert!(queue.is_empty());
}

#[tokio::test]
async fn given_an_item_whose_mark_failed_when_the_queue_reads_it_again_then_the_next_boundary_skips_it_and_marks_it_without_storing_it_twice()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let item = human_item("once", &world);
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.marks = FailingMarks::All;
    let mut session = world.session().await;
    let mut queue = queue_of(std::slice::from_ref(&item));
    run_with(&inbox, &mut session, &world, &mut queue)
        .await
        .unwrap();
    assert!(!world.inbox.is_read(world.agent, item.id()));

    queue
        .read(&world.inbox, &world.context(), limit(5))
        .await
        .unwrap();
    let report = run(&mut session, &world, &mut queue).await.unwrap();

    assert_eq!(report.skipped, [item.id()]);
    assert!(report.stored.is_empty());
    assert!(report.unmarked.is_empty());
    assert!(world.inbox.is_read(world.agent, item.id()));
    assert_eq!(world.access.conversation().turns().len(), 2);
}

#[tokio::test]
async fn given_an_inbox_whose_read_fails_after_a_first_batch_when_the_queue_reads_it_then_the_error_is_returned_and_the_queue_keeps_the_items_it_holds_for_the_next_boundary()
 {
    let world = World::new();
    start_with_trigger(&world).await;
    let items: Vec<InboxItem> = (0..4)
        .map(|n| human_item(&format!("m{n}"), &world))
        .collect();
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.items_before_failing = Some(1);
    let mut queue = Queue::new();

    let error = queue
        .read(&inbox, &world.context(), limit(2))
        .await
        .unwrap_err();

    assert!(!error.is_refusal());
    assert_eq!(queue.items(), &items[..2]);
    let mut session = world.session().await;
    let report = run_with(&inbox, &mut session, &world, &mut queue)
        .await
        .unwrap();
    assert_eq!(report.stored, ids(&items[..2]));
}

#[tokio::test]
async fn given_an_inbox_that_gives_the_same_cursor_twice_when_the_queue_reads_it_then_the_read_is_a_fault_and_the_queue_keeps_the_items_it_holds()
 {
    let world = World::new();
    let items: Vec<InboxItem> = (0..3)
        .map(|n| human_item(&format!("m{n}"), &world))
        .collect();
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.stuck_cursor = true;
    let mut queue = Queue::new();

    let error = queue
        .read(&inbox, &world.context(), limit(2))
        .await
        .unwrap_err();

    assert!(!error.is_refusal());
    assert_eq!(queue.items(), &items[..2]);
}

#[tokio::test]
async fn given_an_inbox_whose_documents_fail_when_the_documents_of_a_mail_are_asked_then_the_inbox_error_is_returned_unchanged_and_not_as_a_refusal()
 {
    let world = World::new();
    let mut session = world.session().await;
    let id = mail_id();
    session
        .add_input_turn(
            br_llm_messages::InputTurn::new(
                turn_id(),
                vec![
                    br_llm_messages::AgentMail::new(
                        id.entry_id(),
                        agent_id(),
                        author("sender"),
                        conversation_id(),
                        vec![text_block("files")],
                    )
                    .unwrap()
                    .into(),
                ],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let mut inbox = FailingInbox::new(&world.inbox);
    inbox.fail_documents = true;

    let error = mail_documents(&session, &inbox, &world.context(), id)
        .await
        .unwrap_err();

    assert!(!error.is_refusal());
    assert_eq!(error.to_string(), "a port failed");
}
