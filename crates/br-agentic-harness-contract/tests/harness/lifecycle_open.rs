use br_agentic_contract::WebAccess;
use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{
    Command, FreshReason, OpenRequest, RunEnd, RunLifecycle, RunMode, RunOutcome, SandboxPort,
    Seed, StartKind,
};

use crate::common::*;
use crate::drivers::close_run;
use crate::lifecycle_driver::Driven;
use crate::scenario::*;

#[tokio::test]
async fn given_a_lifecycle_when_the_run_opens_then_it_takes_the_instance_the_turn_reference_and_the_seed_and_gives_the_start()
-> Res {
    let world = World::new()?;
    let driven = Driven::with_sandbox(&world.sandbox);
    let turn = world.turn_ref(None)?;

    let start = driven
        .open(instance_id()?, turn, seed_of(&[("s.md", b"s")])?)
        .await?;

    assert_eq!(start, StartKind::Fresh(FreshReason::NoSaveEver));
    Ok(())
}

#[tokio::test]
async fn given_no_sandbox_when_the_run_opens_then_nothing_is_opened_and_the_start_is_fresh_with_no_save_ever()
-> Res {
    let world = World::new()?;
    let driven = Driven::without_sandbox();

    let start = driven
        .open(instance_id()?, world.turn_ref(None)?, Seed::empty())
        .await?;

    assert_eq!(start, StartKind::Fresh(FreshReason::NoSaveEver));
    assert!(world.record.calls().is_empty());
    Ok(())
}

#[tokio::test]
async fn given_a_turn_reference_a_seed_and_a_web_access_when_the_run_opens_then_the_open_request_holds_each_unchanged()
-> Res {
    let world = World::new()?;
    let web = WebAccess::Full;
    let driven = Driven::with_sandbox(&world.sandbox).with_web(web.clone());
    let turn = world.turn_ref(Some(turn_id()?))?;
    let seed = seed_of(&[("s.md", b"s"), ("t/u.md", b"u")])?;
    let instance = instance_id()?;

    driven.open(instance, turn, seed.clone()).await?;

    let requests: Vec<OpenRequest> = world
        .record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxOpen { request } => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(
        requests,
        vec![OpenRequest::new(instance, Some(web), turn, seed)]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_run_with_two_background_commands_that_ends_by_failure_or_cancel_when_it_closes_then_the_commands_stop_and_the_box_is_released_and_nothing_is_saved()
-> Res {
    for end in [RunEnd::Failed, RunEnd::Cancelled] {
        let world = World::new()?;
        let driven = Driven::with_sandbox(&world.sandbox);
        let instance = instance_id()?;
        driven
            .open(instance, world.turn_ref(None)?, Seed::empty())
            .await?;
        let sandbox = driven.box_of(instance).ok_or("the box is open")?;
        write_files(&world, &sandbox, &[("a.txt", b"unfinished")]).await?;
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
        assert_eq!(world.sandbox.running(&sandbox).len(), 2);

        driven.close(instance, end).await?;

        let acts: Vec<&str> = world
            .record
            .calls()
            .iter()
            .filter_map(|call| match call {
                HarnessCall::SandboxKill { process, .. } if processes.contains(process) => {
                    Some("kill")
                }
                HarnessCall::SandboxSave { .. } => Some("save"),
                HarnessCall::SandboxRelease { .. } => Some("release"),
                _ => None,
            })
            .collect();
        assert_eq!(acts, vec!["kill", "kill", "release"]);
        assert!(world.sandbox.running(&sandbox).is_empty());
        assert!(world.sandbox.saved(&sandbox).is_none());
        assert!(world.sandbox.is_released(&sandbox));
    }
    Ok(())
}

#[tokio::test]
async fn given_a_run_that_completes_when_it_closes_then_the_next_turn_that_has_it_as_parent_starts_from_its_save()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let opened = open_turn(&world, turn, Seed::empty()).await?;
    write_files(&world, &opened.box_id, &[("a.txt", b"done")]).await?;

    close_run(&world.sandbox, &opened.box_id, &[], RunEnd::Completed).await?;

    let next = open_turn(&world, world.turn_ref(Some(turn.turn))?, Seed::empty()).await?;
    assert_eq!(next.start, StartKind::Continued);
    assert_eq!(
        world.sandbox.files(&next.box_id),
        Some(content(&[("a.txt", b"done")])?)
    );
    Ok(())
}
