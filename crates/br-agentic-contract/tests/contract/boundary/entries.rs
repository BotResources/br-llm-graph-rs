use super::*;

#[tokio::test]
async fn given_a_mail_item_for_a_recipient_in_to_when_the_boundary_stores_it_then_the_entry_is_an_agent_mail_with_the_mail_id()
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

    run(&mut session, &world, &mut queue).await.unwrap();

    let conversation = world.access.conversation();
    let Turn::InputTurn(input) = &conversation.turns()[0] else {
        panic!()
    };
    assert!(
        matches!(&input.entries()[0], IncomingEntry::AgentMail(entry) if entry.id() == id.entry_id())
    );
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
