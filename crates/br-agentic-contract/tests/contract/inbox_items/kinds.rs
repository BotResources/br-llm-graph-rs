use super::*;

#[tokio::test]
async fn given_a_mail_a_human_message_a_notification_and_a_notice_when_the_inbox_is_read_then_all_four_kinds_are_items()
 {
    let world = World::new();
    let sender = agent_id();
    world
        .inbox
        .deliver_mail(sender, new_mail(&[world.agent], &[], "hello"))
        .unwrap();
    world.inbox.deliver_entry(world.agent, human("hi"));
    world.inbox.deliver_entry(world.agent, notification());
    world.inbox.deliver_entry(world.agent, notice("fyi"));

    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    assert_eq!(batch.items.len(), 4);
    assert!(matches!(
        batch.items[0],
        InboxItem::Mail(MailItem::To { .. })
    ));
}

#[tokio::test]
async fn given_an_unread_item_when_the_caller_waits_then_the_wait_returns_at_once() {
    let world = World::new();
    deliver(&world, 1);

    let waited = timeout(Duration::from_secs(1), world.inbox.wait(&world.context())).await;

    assert!(waited.unwrap().is_ok());
}

#[tokio::test]
async fn given_an_item_that_a_read_already_gave_and_that_is_still_unread_when_the_caller_waits_then_the_wait_returns_at_once()
 {
    let world = World::new();
    deliver(&world, 1);
    world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    let waited = timeout(Duration::from_secs(1), world.inbox.wait(&world.context())).await;

    assert!(waited.unwrap().is_ok());
}

#[tokio::test(start_paused = true)]
async fn given_no_unread_item_when_the_caller_waits_then_the_wait_returns_when_an_item_arrives_and_gives_nothing_else()
 {
    let world = World::new();
    let context = world.context();
    let waiting = tokio::spawn({
        let inbox = world.inbox.clone();
        async move { inbox.wait(&context).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!waiting.is_finished());

    deliver(&world, 1);

    let outcome = timeout(Duration::from_secs(1), waiting)
        .await
        .unwrap()
        .unwrap();
    assert!(outcome.is_ok());
}

#[tokio::test(start_paused = true)]
async fn given_a_wait_when_an_item_arrives_for_another_agent_then_the_wait_goes_on() {
    let world = World::new();
    let context = world.context();
    let waiting = tokio::spawn({
        let inbox = world.inbox.clone();
        async move { inbox.wait(&context).await }
    });
    tokio::task::yield_now().await;

    world
        .inbox
        .deliver_entry(agent_id(), human("for someone else"));
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert!(!waiting.is_finished());
    waiting.abort();
}

#[tokio::test(start_paused = true)]
async fn given_a_wait_that_is_dropped_when_an_item_arrives_then_nothing_is_lost() {
    let world = World::new();
    let context = world.context();
    let dropped = timeout(Duration::from_millis(30), world.inbox.wait(&context)).await;
    assert!(dropped.is_err());
    let ids = deliver(&world, 1);

    assert_eq!(all_ids(&world.inbox, &world, 10).await, ids);
}

#[tokio::test]
async fn given_a_mail_a_human_message_a_notification_and_a_notice_when_each_is_marked_as_read_then_none_is_listed_any_more()
 {
    let world = World::new();
    world
        .inbox
        .deliver_mail(agent_id(), new_mail(&[world.agent], &[], "hello"))
        .unwrap();
    world.inbox.deliver_entry(world.agent, human("hi"));
    world.inbox.deliver_entry(world.agent, notification());
    world.inbox.deliver_entry(world.agent, notice("fyi"));
    let batch = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    for item in &batch.items {
        world
            .inbox
            .mark_read(&world.context(), item.id())
            .await
            .unwrap();
        assert!(world.inbox.is_read(world.agent, item.id()));
    }

    let rest = world
        .inbox
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();
    assert!(rest.items.is_empty());
}

#[tokio::test]
async fn given_an_item_already_read_when_it_is_marked_again_then_nothing_changes_and_it_is_not_an_error()
 {
    let world = World::new();
    let ids = deliver(&world, 2);
    world
        .inbox
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    world
        .inbox
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    assert_eq!(all_ids(&world.inbox, &world, 10).await, [ids[1]]);
}

#[tokio::test]
async fn given_an_id_that_the_inbox_does_not_hold_for_the_agent_when_it_is_marked_then_nothing_changes_and_it_is_not_an_error()
 {
    let world = World::new();
    let ids = deliver(&world, 1);
    let other = agent_id();
    world.inbox.deliver_entry(other, human("theirs"));
    let theirs = world
        .inbox
        .items(&world.context_of(other), None, limit(1))
        .await
        .unwrap()
        .items[0]
        .id();

    let unknown = entry_id();
    world
        .inbox
        .mark_read(&world.context(), unknown)
        .await
        .unwrap();
    world
        .inbox
        .mark_read(&world.context(), theirs)
        .await
        .unwrap();

    assert!(!world.inbox.is_read(world.agent, unknown));
    assert!(!world.inbox.is_read(other, theirs));
    assert_eq!(all_ids(&world.inbox, &world, 10).await, ids);
}

#[tokio::test]
async fn given_a_mark_when_the_item_is_read_then_the_only_change_is_its_read_state_the_mail_stays_known()
 {
    let world = World::new();
    let mail = new_mail(&[world.agent], &[], "kept");
    let id = mail.id;
    world.inbox.deliver_mail(agent_id(), mail).unwrap();

    world
        .inbox
        .mark_read(&world.context(), id.entry_id())
        .await
        .unwrap();

    assert!(world.inbox.mail(id).is_some());
}

#[tokio::test]
async fn given_the_inbox_calls_when_the_test_reads_the_record_then_they_carry_the_context_of_the_caller()
 {
    let world = World::new();
    let context = world.context();
    world.inbox.items(&context, None, limit(3)).await.unwrap();

    let recorded = world.record.calls();
    assert!(
        matches!(&recorded[0], Call::InboxItems { context: c, limit: l, .. } if *c == context && l.get() == 3)
    );
}

#[tokio::test]
async fn given_one_item_id_for_two_agents_when_one_agent_marks_it_then_it_stays_unread_for_the_other()
 {
    let world = World::new();
    let other = agent_id();
    let entry = human("shared");
    let id = entry.id();
    world.inbox.deliver_entry(world.agent, entry.clone());
    world.inbox.deliver_entry(other, entry);

    world.inbox.mark_read(&world.context(), id).await.unwrap();

    assert!(world.inbox.is_read(world.agent, id));
    assert!(!world.inbox.is_read(other, id));
    let theirs = world
        .inbox
        .items(&world.context_of(other), None, limit(5))
        .await
        .unwrap();
    assert_eq!(theirs.items.len(), 1);
    assert_eq!(
        all_ids(&world.inbox, &world, 5).await,
        Vec::<EntryId>::new()
    );
}
