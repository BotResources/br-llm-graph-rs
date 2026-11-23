use std::sync::Arc;

use br_agentic_contract::{IncomingContext, IncomingItem, IncomingPort, ThreadAccess};
use br_agentic_harness::{Harness, HarnessHistory};
use br_agentic_harness_contract::memory::HarnessCall;
use br_llm_messages::EntryId;

use crate::common::*;
use crate::doubles::RefusingStored;

struct Rig {
    world: World,
    harness: Harness<Doc>,
    incoming: Arc<dyn IncomingPort>,
    id: EntryId,
}

fn rig() -> Result<Rig, Box<dyn std::error::Error>> {
    let world = World::new()?;
    let harness = world.full().build();
    let incoming = harness.incoming().ok_or("no incoming port")?;
    let mail = new_mail(&[world.agent], "work")?;
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    Ok(Rig {
        world,
        harness,
        incoming,
        id,
    })
}

impl Rig {
    fn read(&self) -> bool {
        self.world
            .mail
            .incoming()
            .is_read(self.world.agent, self.id)
    }

    async fn listed(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let batch = self
            .incoming
            .items(&self.world.context(), None, limit(10)?)
            .await?;
        Ok(batch.items.iter().any(|item| item.id() == self.id))
    }

    async fn write(&self) -> Res {
        self.harness
            .history()
            .add_input_turn(&self.world.thread, mail_turn(self.id)?)
            .await?;
        Ok(())
    }

    async fn mark(&self) -> Res {
        self.incoming
            .mark_read(&self.world.context(), self.id)
            .await?;
        settle().await;
        Ok(())
    }
}

#[tokio::test]
async fn given_an_entry_that_is_stored_when_the_node_acknowledges_then_the_mail_is_marked_read_and_only_after_the_stored_signal()
-> Res {
    let rig = rig()?;
    rig.write().await?;

    rig.mark().await?;

    assert!(rig.read());
    assert!(!rig.listed().await?);
    let order: Vec<&str> = rig
        .world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            HarnessCall::Stored { .. } => Some("stored"),
            HarnessCall::MailMarkRead { .. } => Some("mark"),
            _ => None,
        })
        .collect();
    assert_eq!(order, vec!["stored", "mark"]);
    Ok(())
}

#[tokio::test]
async fn given_an_entry_not_stored_yet_when_the_node_acknowledges_then_the_call_returns_at_once_and_the_mark_comes_when_the_signal_comes()
-> Res {
    let rig = rig()?;
    rig.world.history.expect(rig.id);

    rig.mark().await?;
    assert!(!rig.read());
    assert!(rig.listed().await?);

    rig.write().await?;
    settle().await;

    assert!(rig.read());
    Ok(())
}

#[tokio::test]
async fn given_an_entry_the_history_dropped_when_the_node_acknowledges_then_the_mail_is_not_marked_and_it_can_come_again()
-> Res {
    let rig = rig()?;
    rig.world.history.expect(rig.id);
    rig.world.history.drop_entry(rig.id);

    rig.mark().await?;

    assert!(!rig.read());
    assert!(rig.listed().await?);
    assert!(
        !rig.world
            .record
            .calls()
            .iter()
            .any(|call| matches!(call, HarnessCall::MailMarkRead { .. }))
    );
    Ok(())
}

#[tokio::test]
async fn given_an_entry_the_history_has_no_record_of_when_the_node_acknowledges_then_the_unknown_answer_is_never_stored_and_the_mail_stays_unread()
-> Res {
    let rig = rig()?;

    rig.mark().await?;

    assert!(!rig.read());
    assert!(rig.listed().await?);
    Ok(())
}

#[tokio::test]
async fn given_a_thread_the_history_has_no_slot_for_when_the_node_acknowledges_then_the_mail_stays_unread()
-> Res {
    let rig = rig()?;
    rig.write().await?;
    let elsewhere = IncomingContext {
        thread: thread_ref()?,
        ..rig.world.context()
    };

    rig.incoming.mark_read(&elsewhere, rig.id).await?;
    settle().await;

    assert!(!rig.read());
    Ok(())
}

#[tokio::test]
async fn given_a_change_channel_that_closes_before_the_entry_is_stored_when_the_node_acknowledges_then_the_mail_stays_unread()
-> Res {
    let rig = rig()?;
    rig.world.history.expect(rig.id);
    rig.world.history.close_channel();

    rig.mark().await?;

    assert!(!rig.read());
    assert!(rig.listed().await?);
    Ok(())
}

#[tokio::test]
async fn given_a_stored_signal_that_returns_a_refusal_when_the_node_acknowledges_then_the_mail_stays_unread()
-> Res {
    let world = World::new()?;
    let refusing = RefusingStored(world.history.clone());
    let harness: Harness<Doc> = Harness::builder(
        Arc::new(refusing) as Arc<dyn HarnessHistory>,
        Arc::new(FixedPrompt(String::new())),
        Arc::new(br_agentic_contract::memory::FixedToolsPort::new(
            &world.contract_record,
            Vec::new(),
        )),
    )
    .mail(Arc::new(world.mail.clone()))
    .build();
    let mail = new_mail(&[world.agent], "work")?;
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    world
        .history
        .add_input_turn(&world.thread, mail_turn(id)?)
        .await?;

    let incoming = harness.incoming().ok_or("no incoming port")?;
    incoming.mark_read(&world.context(), id).await?;
    settle().await;

    assert!(!world.mail.incoming().is_read(world.agent, id));
    Ok(())
}

#[tokio::test]
async fn given_an_entry_written_and_no_mark_made_when_the_run_crashes_then_the_mail_is_delivered_again_and_not_lost()
-> Res {
    let rig = rig()?;
    rig.write().await?;

    assert!(rig.listed().await?);
    assert!(!rig.read());
    Ok(())
}

#[tokio::test]
async fn given_a_wait_in_progress_when_the_harness_is_dropped_then_the_wait_ends_and_a_later_store_marks_nothing()
-> Res {
    let rig = rig()?;
    rig.world.history.expect(rig.id);
    rig.mark().await?;
    let Rig {
        world,
        harness,
        incoming,
        id,
    } = rig;
    drop(incoming);
    drop(harness);

    world
        .history
        .add_input_turn(&world.thread, mail_turn(id)?)
        .await?;
    settle().await;

    assert!(!world.mail.incoming().is_read(world.agent, id));
    Ok(())
}

#[tokio::test]
async fn given_a_mail_port_when_the_node_lists_the_unread_items_through_the_harness_then_it_gets_the_batch_of_the_mail_port()
-> Res {
    let rig = rig()?;

    let batch = rig
        .incoming
        .items(&rig.world.context(), None, limit(10)?)
        .await?;

    assert_eq!(batch.items.len(), 1);
    assert!(matches!(&batch.items[0], IncomingItem::Mail(item) if item.id().entry_id() == rig.id));
    Ok(())
}
