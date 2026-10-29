use br_agentic_contract::{
    Incoming, MailItem, Stored, ThreadAccess, ThreadError, TurnTarget, WrittenEntry,
};
use br_llm_messages::{ConversationId, IncomingEntry, Turn};

use crate::common::*;

#[tokio::test]
async fn given_an_empty_thread_when_an_input_turn_is_added_then_the_projection_holds_its_entries_in_order()
 {
    let world = World::new();
    let first = human("one");
    let second = notice("two");
    let turn = turn_id();

    let stored = world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn,
            vec![
                Incoming::Entry(first.clone()),
                Incoming::Entry(second.clone()),
            ],
        )
        .await
        .unwrap();

    assert_eq!(stored, Stored::Added);
    assert_eq!(
        entry_ids(&world.projected().await),
        vec![first.id(), second.id()]
    );
}

#[tokio::test]
async fn given_an_input_turn_when_an_entry_is_added_at_its_end_then_it_comes_last() {
    let world = World::new();
    let turn = world.start_with_human("one").await;
    let late = notice("late");

    world
        .access
        .add_incoming(
            &world.thread,
            world.agent,
            TurnTarget::Input(turn),
            Incoming::Entry(late.clone()),
        )
        .await
        .unwrap();

    assert_eq!(
        world
            .projected()
            .await
            .entries()
            .last()
            .unwrap()
            .entry()
            .id(),
        late.id()
    );
}

#[tokio::test]
async fn given_a_new_ai_turn_when_its_first_entry_is_added_then_the_turn_is_created_with_the_author_and_agent()
 {
    let world = World::new();
    world.start_with_human("hello").await;
    let turn = turn_id();

    world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry_id(), "hi"),
        )
        .await
        .unwrap();

    let projected = world.projected().await;
    let ai = projected.turns().last().and_then(Turn::as_ai).unwrap();
    assert_eq!(ai.id(), turn);
    assert_eq!(ai.agent(), world.agent);
    assert_eq!(ai.author(), &world.name);
}

#[tokio::test]
async fn given_a_mail_item_to_a_recipient_when_it_is_added_then_an_agent_mail_with_the_mail_id_and_no_attachment_name_is_stored()
 {
    let world = World::new();
    let sender = agent_id();
    world.hub.set_author(sender, author("sender"));
    let item = mail_item_to(sender, &[world.agent], "plan", &["secret-attachment.txt"]);
    let id = match &item {
        MailItem::To { id, .. } => id.entry_id(),
        MailItem::Cc { .. } => unreachable!(),
    };
    let turn = turn_id();

    world
        .access
        .add_input_turn(&world.thread, world.agent, turn, vec![Incoming::Mail(item)])
        .await
        .unwrap();

    match incoming_of(&world.projected().await, id) {
        IncomingEntry::AgentMail(mail) => {
            assert_eq!(mail.sender(), sender);
            assert_eq!(mail.author(), &author("sender"));
            let text = format!("{:?}", mail.content());
            assert!(text.contains("plan"));
            assert!(!text.contains("secret-attachment.txt"));
        }
        other => panic!("expected an agent mail, got {other:?}"),
    }
}

#[tokio::test]
async fn given_a_mail_item_to_a_copy_recipient_when_it_is_added_then_a_runtime_notice_with_the_mail_id_is_stored()
 {
    let world = World::new();
    let id = mail_id();
    let item = MailItem::Cc {
        id,
        subject: "fyi".to_owned(),
    };

    world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Mail(item)],
        )
        .await
        .unwrap();

    match incoming_of(&world.projected().await, id.entry_id()) {
        IncomingEntry::RuntimeNotice(notice) => assert!(notice.text().contains("fyi")),
        other => panic!("expected a runtime notice, got {other:?}"),
    }
}

#[tokio::test]
async fn given_an_entry_written_by_the_instance_when_it_is_added_then_it_is_an_agent_mail_with_the_given_author_and_the_source_conversation_filled()
 {
    let world = World::new();
    let written = WrittenEntry {
        id: entry_id(),
        sender: world.agent,
        author: author("critic"),
        content: vec![text_block("be stricter")],
    };
    let id = written.id;
    let source = ConversationId::from_uuid(uuid()).unwrap();
    world.hub.set_source(world.agent, source);

    world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Written(written)],
        )
        .await
        .unwrap();

    match incoming_of(&world.projected().await, id) {
        IncomingEntry::AgentMail(mail) => {
            assert_eq!(mail.author(), &author("critic"));
            assert_eq!(mail.source_conversation(), source);
        }
        other => panic!("expected an agent mail, got {other:?}"),
    }
}

#[tokio::test]
async fn given_an_addition_whose_identity_the_thread_holds_when_it_is_added_again_then_it_is_accepted_and_nothing_changes()
 {
    let world = World::new();
    let turn = world.start_with_human("one").await;
    let entry = notice("again");
    let item = Incoming::Entry(entry.clone());
    let target = TurnTarget::Input(turn);
    world
        .access
        .add_incoming(&world.thread, world.agent, target.clone(), item.clone())
        .await
        .unwrap();
    let before = world.hub.conversation(&world.thread).unwrap();

    let stored = world
        .access
        .add_incoming(&world.thread, world.agent, target, item)
        .await
        .unwrap();

    assert_eq!(stored, Stored::Held);
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
}

#[tokio::test]
async fn given_a_held_identity_and_a_target_the_rules_would_refuse_when_it_is_added_then_the_held_check_wins()
 {
    let world = World::new();
    let turn = world.start_with_human("one").await;
    let entry = notice("held");
    world
        .access
        .add_incoming(
            &world.thread,
            world.agent,
            TurnTarget::Input(turn),
            Incoming::Entry(entry.clone()),
        )
        .await
        .unwrap();

    let stored = world
        .access
        .add_incoming(
            &world.thread,
            world.agent,
            TurnTarget::Input(turn_id()),
            Incoming::Entry(entry),
        )
        .await
        .unwrap();

    assert_eq!(stored, Stored::Held);
}

#[tokio::test]
async fn given_a_new_addition_that_the_rules_refuse_when_it_is_added_then_it_is_refused_with_a_reason_and_the_thread_is_unchanged()
 {
    let world = World::new();
    world.start_with_human("one").await;
    let before = world.hub.conversation(&world.thread).unwrap();

    let error = world
        .access
        .add_incoming(
            &world.thread,
            world.agent,
            TurnTarget::Input(turn_id()),
            Incoming::Entry(notice("lost")),
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ThreadError::Refused(reason) if !reason.as_str().is_empty()));
    assert_eq!(world.hub.conversation(&world.thread).unwrap(), before);
}

#[tokio::test]
async fn given_an_unknown_thread_when_it_is_loaded_then_the_port_faults() {
    let world = World::new();
    let other = World::new();

    let error = world.access.load(&other.thread, world.agent).await;

    assert!(error.is_err());
}

#[tokio::test]
async fn given_a_sender_without_an_author_name_when_its_mail_is_added_then_the_thread_access_faults_and_nothing_is_stored()
 {
    let world = World::new();
    let nameless = agent_id();
    let item = mail_item_to(nameless, &[world.agent], "x", &[]);

    let error = world
        .access
        .add_input_turn(
            &world.thread,
            world.agent,
            turn_id(),
            vec![Incoming::Mail(item)],
        )
        .await
        .unwrap_err();

    assert!(matches!(error, ThreadError::Fault(_)));
    assert!(world.projected().await.turns().is_empty());
}

#[tokio::test]
async fn given_two_agents_when_the_projection_is_loaded_for_each_then_an_ai_turn_of_the_other_is_an_input_turn()
 {
    let world = World::new();
    let other = agent_id();
    world.start_with_human("hello").await;
    let turn = turn_id();
    world
        .access
        .complete_response(
            &world.thread,
            world.agent,
            world.ai(turn),
            response_text(entry_id(), "visible"),
        )
        .await
        .unwrap();

    let own = world.projected().await;
    let foreign = world.projected_of(other).await;

    assert!(matches!(own.turn(&turn).unwrap(), Turn::AITurn(_)));
    assert!(matches!(foreign.turn(&turn).unwrap(), Turn::InputTurn(_)));
}
