use br_agentic_contract::{BoundaryError, MailId, MailItem};
use br_llm_messages::{AgentId, AgentMail, Author, ConversationId, Error, RuntimeNotice};

use super::*;

async fn run_with(
    world: &World,
    to: &[AgentId],
    cc: &[AgentId],
    builder: &(dyn Fn(&MailItem) -> Result<IncomingEntry, Error> + Sync),
) -> (BoundaryError, MailId, Queue) {
    let mail = new_mail(to, cc, "hello");
    let id = mail.id;
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);
    let error = store_at_boundary(
        &mut session,
        &world.inbox,
        &world.context(),
        &mut queue,
        builder,
        turn_id(),
    )
    .await
    .unwrap_err();
    (error, id, queue)
}

fn agent_mail_with(id: EntryId, item: &MailItem) -> IncomingEntry {
    let MailItem::To {
        sender,
        author,
        source,
        subject,
        ..
    } = item
    else {
        panic!("a mail in to")
    };
    IncomingEntry::from(
        AgentMail::new(
            id,
            *sender,
            author.clone(),
            *source,
            vec![text_block(subject)],
        )
        .unwrap(),
    )
}

fn notice_with(id: EntryId) -> IncomingEntry {
    IncomingEntry::from(RuntimeNotice::new(id, "notice").unwrap())
}

fn assert_refused(world: &World, error: &BoundaryError, id: MailId, queue: &Queue) {
    assert!(matches!(error.fault, SessionError::WrongMailEntry(m) if m == id));
    assert!(error.done.stored.is_empty());
    assert!(marks(world).is_empty());
    assert!(!world.inbox.is_read(world.agent, id.entry_id()));
    assert_eq!(queue.len(), 1);
    assert!(world.access.conversation().turns().is_empty());
}

#[tokio::test]
async fn given_a_mail_builder_that_gives_an_entry_with_another_id_than_the_mail_id_when_the_boundary_runs_then_the_boundary_faults_and_nothing_is_stored_or_marked()
 {
    let world = World::new();
    let builder = |item: &MailItem| Ok(agent_mail_with(entry_id(), item));

    let (error, id, queue) = run_with(&world, &[world.agent], &[], &builder).await;

    assert_refused(&world, &error, id, &queue);
}

#[tokio::test]
async fn given_a_mail_builder_that_gives_a_runtime_notice_for_a_recipient_in_to_when_the_boundary_runs_then_the_boundary_faults_and_nothing_is_stored_or_marked()
 {
    let world = World::new();
    let builder = |item: &MailItem| Ok(notice_with(item.id().entry_id()));

    let (error, id, queue) = run_with(&world, &[world.agent], &[], &builder).await;

    assert_refused(&world, &error, id, &queue);
}

#[tokio::test]
async fn given_a_mail_builder_that_gives_an_agent_mail_for_a_recipient_in_cc_when_the_boundary_runs_then_the_boundary_faults_and_nothing_is_stored_or_marked()
 {
    let world = World::new();
    let sender = agent_id();
    let builder = move |item: &MailItem| {
        Ok(IncomingEntry::from(
            AgentMail::new(
                item.id().entry_id(),
                sender,
                author("sender"),
                conversation_id(),
                vec![text_block("fyi")],
            )
            .unwrap(),
        ))
    };

    let (error, id, queue) = run_with(&world, &[agent_id()], &[world.agent], &builder).await;

    assert_refused(&world, &error, id, &queue);
}

#[tokio::test]
async fn given_a_mail_builder_that_gives_a_runtime_notice_with_another_id_for_a_recipient_in_cc_when_the_boundary_runs_then_the_boundary_faults_and_nothing_is_stored_or_marked()
 {
    let world = World::new();
    let builder = |_: &MailItem| Ok(notice_with(entry_id()));

    let (error, id, queue) = run_with(&world, &[agent_id()], &[world.agent], &builder).await;

    assert_refused(&world, &error, id, &queue);
}

fn agent_mail_changing(
    item: &MailItem,
    change: impl FnOnce(&mut AgentId, &mut Author, &mut ConversationId),
) -> IncomingEntry {
    let MailItem::To {
        id,
        sender,
        author,
        source,
        subject,
        ..
    } = item
    else {
        panic!("a mail in to")
    };
    let (mut sender, mut author, mut source) = (*sender, author.clone(), *source);
    change(&mut sender, &mut author, &mut source);
    IncomingEntry::from(
        AgentMail::new(
            id.entry_id(),
            sender,
            author,
            source,
            vec![text_block(subject)],
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn given_a_mail_builder_that_gives_an_agent_mail_with_another_sender_than_the_item_when_the_boundary_runs_then_the_boundary_faults_and_nothing_is_stored_or_marked()
 {
    let world = World::new();
    let builder = |item: &MailItem| {
        Ok(agent_mail_changing(item, |sender, _, _| {
            *sender = agent_id()
        }))
    };

    let (error, id, queue) = run_with(&world, &[world.agent], &[], &builder).await;

    assert_refused(&world, &error, id, &queue);
}

#[tokio::test]
async fn given_a_mail_builder_that_gives_an_agent_mail_with_another_author_name_than_the_item_when_the_boundary_runs_then_the_boundary_faults_and_nothing_is_stored_or_marked()
 {
    let world = World::new();
    let builder = |item: &MailItem| {
        Ok(agent_mail_changing(item, |_, author, _| {
            *author = super::author("somebody else")
        }))
    };

    let (error, id, queue) = run_with(&world, &[world.agent], &[], &builder).await;

    assert_refused(&world, &error, id, &queue);
}

#[tokio::test]
async fn given_a_mail_builder_that_gives_an_agent_mail_with_another_source_conversation_than_the_item_when_the_boundary_runs_then_the_boundary_faults_and_nothing_is_stored_or_marked()
 {
    let world = World::new();
    let builder = |item: &MailItem| {
        Ok(agent_mail_changing(item, |_, _, source| {
            *source = conversation_id()
        }))
    };

    let (error, id, queue) = run_with(&world, &[world.agent], &[], &builder).await;

    assert_refused(&world, &error, id, &queue);
}

#[tokio::test]
async fn given_a_mail_builder_that_copies_the_sender_the_author_name_and_the_source_of_the_item_when_the_boundary_runs_then_the_entry_is_stored_and_the_item_is_marked()
 {
    let world = World::new();
    let mail = new_mail(&[world.agent], &[], "hello");
    let id = mail.id;
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);
    let builder = |item: &MailItem| Ok(agent_mail_changing(item, |_, _, _| {}));

    let report = store_at_boundary(
        &mut session,
        &world.inbox,
        &world.context(),
        &mut queue,
        &builder,
        turn_id(),
    )
    .await
    .unwrap();

    assert_eq!(report.stored, vec![id.entry_id()]);
    assert!(world.inbox.is_read(world.agent, id.entry_id()));
}

#[tokio::test]
async fn given_a_faulty_mail_builder_when_the_queue_also_holds_a_human_message_then_that_item_is_not_stored_or_marked_either_and_both_stay_queued()
 {
    let world = World::new();
    let human = human_item("hi", &world);
    let mail = new_mail(&[world.agent], &[], "hello");
    world.inbox.deliver_mail(agent_id(), mail).unwrap();
    let batch = world
        .inbox
        .items(&world.context(), None, limit(5))
        .await
        .unwrap();
    let mut session = world.session().await;
    let mut queue = queue_of(&batch.items);
    let builder = |item: &MailItem| Ok(agent_mail_with(entry_id(), item));

    let error = store_at_boundary(
        &mut session,
        &world.inbox,
        &world.context(),
        &mut queue,
        &builder,
        turn_id(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error.fault, SessionError::WrongMailEntry(_)));
    assert!(marks(&world).is_empty());
    assert!(!world.inbox.is_read(world.agent, human.id()));
    assert_eq!(queue.len(), 2);
}
