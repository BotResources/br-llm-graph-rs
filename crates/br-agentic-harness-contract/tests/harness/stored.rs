use std::time::Duration;

use br_agentic_contract::ThreadAccess;
use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{HarnessHistory, StoredAnswer};
use tokio::time::timeout;

use crate::common::*;

const SHORT: Duration = Duration::from_millis(30);
const LONG: Duration = Duration::from_secs(2);

#[tokio::test]
async fn given_an_entry_written_through_the_thread_access_when_the_signal_is_asked_then_it_answers_stored()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    world
        .history
        .add_entry(&world.thread, world.turn()?, world.response_entry(entry)?)
        .await?;

    let answer = world.history.stored(&world.thread, entry).await?;

    assert_eq!(answer, StoredAnswer::Stored);
    Ok(())
}

#[tokio::test]
async fn given_an_adapter_built_for_the_run_when_it_is_asked_about_an_entry_it_has_no_record_of_then_it_is_a_fault_and_never_stored()
-> Res {
    let world = World::new()?;

    let answer = world.history.stored(&world.thread, entry_id()?).await;

    assert!(matches!(answer, Err(ref error) if !error.is_refusal()));
    Ok(())
}

#[tokio::test]
async fn given_a_thread_the_adapter_has_no_slot_for_when_the_signal_is_asked_then_it_is_a_fault_and_never_stored()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    world
        .history
        .add_entry(&world.thread, world.turn()?, world.response_entry(entry)?)
        .await?;

    let answer = world.history.stored(&thread_ref()?, entry).await;

    assert!(answer.is_err());
    Ok(())
}

#[tokio::test]
async fn given_an_entry_that_will_never_be_stored_when_the_signal_is_asked_then_it_answers_dropped()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    world.history.expect(entry);
    world.history.drop_entry(entry);

    let answer = world.history.stored(&world.thread, entry).await?;

    assert_eq!(answer, StoredAnswer::Dropped);
    Ok(())
}

#[tokio::test]
async fn given_an_entry_not_stored_yet_when_it_is_stored_later_then_the_wait_ends_with_stored_and_the_node_was_free_meanwhile()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    world.history.expect(entry);
    let waiting = world.history.stored(&world.thread, entry);
    tokio::pin!(waiting);

    assert!(timeout(SHORT, &mut waiting).await.is_err());
    world
        .history
        .add_entry(
            &world.thread,
            world.turn()?,
            world.response_entry(entry_id()?)?,
        )
        .await?;
    assert!(timeout(SHORT, &mut waiting).await.is_err());
    world
        .history
        .add_entry(&world.thread, world.turn()?, world.response_entry(entry)?)
        .await?;

    assert_eq!(timeout(LONG, &mut waiting).await??, StoredAnswer::Stored);
    Ok(())
}

#[tokio::test]
async fn given_an_entry_not_stored_yet_when_it_is_dropped_later_then_the_wait_ends_with_dropped()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    world.history.expect(entry);
    let waiting = world.history.stored(&world.thread, entry);
    tokio::pin!(waiting);
    assert!(timeout(SHORT, &mut waiting).await.is_err());

    world.history.drop_entry(entry);

    assert_eq!(timeout(LONG, &mut waiting).await??, StoredAnswer::Dropped);
    Ok(())
}

#[tokio::test]
async fn given_a_wait_when_the_change_channel_closes_before_the_entry_is_stored_then_the_wait_is_a_fault_and_not_stored()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    world.history.expect(entry);
    let waiting = world.history.stored(&world.thread, entry);
    tokio::pin!(waiting);
    assert!(timeout(SHORT, &mut waiting).await.is_err());

    world.history.close_channel();

    assert!(timeout(LONG, &mut waiting).await?.is_err());
    Ok(())
}

#[tokio::test]
async fn given_a_wait_that_was_dropped_when_the_entry_is_stored_then_a_new_wait_still_answers_stored()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    world.history.expect(entry);
    assert!(
        timeout(SHORT, world.history.stored(&world.thread, entry))
            .await
            .is_err()
    );

    world
        .history
        .add_entry(&world.thread, world.turn()?, world.response_entry(entry)?)
        .await?;

    let answer = world.history.stored(&world.thread, entry).await?;
    assert_eq!(answer, StoredAnswer::Stored);
    Ok(())
}

#[tokio::test]
async fn given_a_history_when_it_is_used_as_the_node_history_then_it_serves_every_thread_access_operation_and_the_signal_is_recorded()
-> Res {
    let world = World::new()?;
    let entry = entry_id()?;
    let node_view: &dyn ThreadAccess = &world.history;
    node_view
        .add_entry(&world.thread, world.turn()?, world.response_entry(entry)?)
        .await?;
    let conversation = node_view.read(&world.thread).await?;
    assert!(conversation.entry(&entry).is_ok());

    world.history.stored(&world.thread, entry).await?;

    let asked =
        world.record.calls().iter().any(
            |call| matches!(call, HarnessCall::Stored { entry: asked, .. } if *asked == entry),
        );
    assert!(asked);
    Ok(())
}
