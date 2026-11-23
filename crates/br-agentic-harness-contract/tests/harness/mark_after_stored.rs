use br_agentic_contract::IncomingItem;
use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{HarnessHistory, MailPort, ThreadAccess};

use crate::common::*;
use crate::drivers::mark_when_stored;

async fn unread(
    world: &World,
    recipient: br_llm_messages::AgentId,
) -> Result<Vec<br_llm_messages::EntryId>, Box<dyn std::error::Error>> {
    let batch = world
        .mail
        .items(&world.context_of(recipient)?, None, limit(10)?)
        .await?;
    Ok(batch.items.iter().map(IncomingItem::id).collect())
}

async fn delivered(
    world: &World,
) -> Result<(br_llm_messages::AgentId, br_llm_messages::EntryId), Box<dyn std::error::Error>> {
    let recipient = agent_id()?;
    let mail = new_mail(&[recipient], &[], "work")?;
    let entry = mail.id.entry_id();
    world.mail.send(&world.context()?, mail).await?;
    Ok((recipient, entry))
}

#[tokio::test]
async fn given_a_mail_whose_entry_is_stored_when_the_harness_follows_the_signal_then_the_mail_is_marked_read_after_the_signal()
-> Res {
    let world = World::new()?;
    let (recipient, entry) = delivered(&world).await?;
    world
        .history
        .add_entry(&world.thread, world.turn()?, world.response_entry(entry)?)
        .await?;

    let marked = mark_when_stored(
        &world.history,
        &world.mail,
        &world.context_of(recipient)?,
        entry,
    )
    .await?;

    assert!(marked);
    assert!(unread(&world, recipient).await?.is_empty());
    let calls = world.record.calls();
    let signal = calls
        .iter()
        .position(|call| matches!(call, HarnessCall::Stored { .. }));
    let mark = calls
        .iter()
        .position(|call| matches!(call, HarnessCall::MailMarkRead { .. }));
    assert!(signal < mark && mark.is_some());
    Ok(())
}

#[tokio::test]
async fn given_a_mail_whose_entry_is_dropped_when_the_harness_follows_the_signal_then_the_mail_is_not_marked_and_stays_unread()
-> Res {
    let world = World::new()?;
    let (recipient, entry) = delivered(&world).await?;
    world.history.expect(entry);
    world.history.drop_entry(entry);

    let marked = mark_when_stored(
        &world.history,
        &world.mail,
        &world.context_of(recipient)?,
        entry,
    )
    .await?;

    assert!(!marked);
    assert_eq!(unread(&world, recipient).await?, vec![entry]);
    assert!(
        !world
            .record
            .calls()
            .iter()
            .any(|call| matches!(call, HarnessCall::MailMarkRead { .. }))
    );
    Ok(())
}

#[tokio::test]
async fn given_a_signal_that_is_a_fault_when_the_harness_follows_it_then_the_mail_is_not_marked_and_stays_unread()
-> Res {
    let world = World::new()?;
    let (recipient, entry) = delivered(&world).await?;

    let marked = mark_when_stored(
        &world.history,
        &world.mail,
        &world.context_of(recipient)?,
        entry,
    )
    .await;

    assert!(marked.is_err());
    assert_eq!(unread(&world, recipient).await?, vec![entry]);
    assert!(
        !world
            .record
            .calls()
            .iter()
            .any(|call| matches!(call, HarnessCall::MailMarkRead { .. }))
    );
    Ok(())
}

#[tokio::test]
async fn given_a_crash_between_the_store_and_the_mark_when_the_mail_is_read_again_then_it_is_delivered_a_second_time_and_never_lost()
-> Res {
    let world = World::new()?;
    let (recipient, entry) = delivered(&world).await?;
    world
        .history
        .add_entry(&world.thread, world.turn()?, world.response_entry(entry)?)
        .await?;
    let stored = world.history.stored(&world.thread, entry).await?;
    assert_eq!(stored, br_agentic_harness_contract::StoredAnswer::Stored);

    assert_eq!(unread(&world, recipient).await?, vec![entry]);
    Ok(())
}
