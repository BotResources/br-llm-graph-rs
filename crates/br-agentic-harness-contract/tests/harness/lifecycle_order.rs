use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{Command, RunMode, RunOutcome, SandboxPort};

use crate::common::*;
use crate::drivers::close_run;

#[tokio::test]
async fn given_a_run_with_two_background_commands_when_it_closes_then_the_commands_stop_then_the_folder_is_saved_then_the_box_is_released()
-> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;
    let mut processes = Vec::new();
    for text in ["server-one", "server-two"] {
        if let RunOutcome::Started(process) = world
            .sandbox
            .run(&sandbox, &Command::new(text)?, RunMode::Background)
            .await?
        {
            processes.push(process);
        }
    }

    close_run(&world.sandbox, &sandbox, &processes).await?;

    let order: Vec<&str> = world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxKill { .. } => Some("kill"),
            HarnessCall::SandboxSave { .. } => Some("save"),
            HarnessCall::SandboxRelease { .. } => Some("release"),
            _ => None,
        })
        .collect();
    assert_eq!(order, vec!["kill", "kill", "save", "release"]);
    assert!(world.sandbox.running(&sandbox).is_empty());
    assert!(world.sandbox.saved(&sandbox).is_some());
    assert!(world.sandbox.is_released(&sandbox));
    Ok(())
}

#[tokio::test]
async fn given_a_box_released_before_the_save_when_the_save_is_called_then_the_provider_refuses_so_the_wrong_order_cannot_pass()
-> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;

    world.sandbox.release(&sandbox).await?;
    let saved = world.sandbox.save(&sandbox).await;

    assert!(saved.is_err());
    assert!(world.sandbox.saved(&sandbox).is_none());
    Ok(())
}
