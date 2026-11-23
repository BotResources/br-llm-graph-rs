use super::*;

#[tokio::test]
async fn given_a_mail_a_human_message_a_notification_and_a_notice_when_the_incoming_port_is_read_then_all_four_kinds_are_items()
 {
    let world = World::new();
    let sender = agent_id();
    world
        .incoming
        .deliver_mail(sender, new_mail(&[world.agent], &[], "hello"))
        .unwrap();
    world.incoming.deliver_entry(world.agent, human("hi"));
    world.incoming.deliver_entry(world.agent, notification());
    world.incoming.deliver_entry(world.agent, notice("fyi"));

    let batch = world
        .incoming
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    assert_eq!(batch.items.len(), 4);
    assert!(matches!(
        batch.items[0],
        IncomingItem::Mail(MailItem::To { .. })
    ));
}

#[tokio::test]
async fn given_a_mail_a_human_message_a_notification_and_a_notice_when_each_is_marked_as_read_then_none_is_listed_any_more()
 {
    let world = World::new();
    world
        .incoming
        .deliver_mail(agent_id(), new_mail(&[world.agent], &[], "hello"))
        .unwrap();
    world.incoming.deliver_entry(world.agent, human("hi"));
    world.incoming.deliver_entry(world.agent, notification());
    world.incoming.deliver_entry(world.agent, notice("fyi"));
    let batch = world
        .incoming
        .items(&world.context(), None, limit(10))
        .await
        .unwrap();

    for item in &batch.items {
        world
            .incoming
            .mark_read(&world.context(), item.id())
            .await
            .unwrap();
        assert!(world.incoming.is_read(world.agent, item.id()));
    }

    let rest = world
        .incoming
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
        .incoming
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    world
        .incoming
        .mark_read(&world.context(), ids[0])
        .await
        .unwrap();

    assert_eq!(all_ids(&world.incoming, &world, 10).await, [ids[1]]);
}

#[tokio::test]
async fn given_an_id_that_the_incoming_port_does_not_hold_for_the_agent_when_it_is_marked_then_nothing_changes_and_it_is_not_an_error()
 {
    let world = World::new();
    let ids = deliver(&world, 1);
    let other = agent_id();
    world.incoming.deliver_entry(other, human("theirs"));
    let theirs = world
        .incoming
        .items(&world.context_of(other), None, limit(1))
        .await
        .unwrap()
        .items[0]
        .id();

    let unknown = entry_id();
    world
        .incoming
        .mark_read(&world.context(), unknown)
        .await
        .unwrap();
    world
        .incoming
        .mark_read(&world.context(), theirs)
        .await
        .unwrap();

    assert!(!world.incoming.is_read(world.agent, unknown));
    assert!(!world.incoming.is_read(other, theirs));
    assert_eq!(all_ids(&world.incoming, &world, 10).await, ids);
}

#[tokio::test]
async fn given_a_mark_when_the_item_is_read_then_the_only_change_is_its_read_state_the_mail_stays_known()
 {
    let world = World::new();
    let mail = new_mail(&[world.agent], &[], "kept");
    let id = mail.id;
    world.incoming.deliver_mail(agent_id(), mail).unwrap();

    world
        .incoming
        .mark_read(&world.context(), id.entry_id())
        .await
        .unwrap();

    assert!(world.incoming.mail(id).is_some());
}

#[tokio::test]
async fn given_the_incoming_port_calls_when_the_test_reads_the_record_then_they_carry_the_context_of_the_caller()
 {
    let world = World::new();
    let context = world.context();
    world
        .incoming
        .items(&context, None, limit(3))
        .await
        .unwrap();

    let recorded = world.record.calls();
    assert!(
        matches!(&recorded[0], Call::IncomingItems { context: c, limit: l, .. } if *c == context && l.get() == 3)
    );
}

#[tokio::test]
async fn given_one_item_id_for_two_agents_when_one_agent_marks_it_then_it_stays_unread_for_the_other()
 {
    let world = World::new();
    let other = agent_id();
    let entry = human("shared");
    let id = entry.id();
    world.incoming.deliver_entry(world.agent, entry.clone());
    world.incoming.deliver_entry(other, entry);

    world
        .incoming
        .mark_read(&world.context(), id)
        .await
        .unwrap();

    assert!(world.incoming.is_read(world.agent, id));
    assert!(!world.incoming.is_read(other, id));
    let theirs = world
        .incoming
        .items(&world.context_of(other), None, limit(5))
        .await
        .unwrap();
    assert_eq!(theirs.items.len(), 1);
    assert_eq!(
        all_ids(&world.incoming, &world, 5).await,
        Vec::<EntryId>::new()
    );
}
