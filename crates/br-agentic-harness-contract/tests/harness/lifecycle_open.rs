use br_agentic_contract::{InstanceId, PortError, PortFuture, WebAccess};
use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{
    FreshReason, OpenRequest, RunEnd, RunLifecycle, SandboxPort, Seed, StartKind, TurnRef,
};

use crate::common::*;
use crate::drivers::close_run;
use crate::scenario::*;

struct Driven<'s> {
    sandbox: Option<&'s dyn SandboxPort>,
    web: Option<WebAccess>,
}

impl RunLifecycle for Driven<'_> {
    fn open<'a>(
        &'a self,
        instance: InstanceId,
        turn: TurnRef,
        seed: Seed,
    ) -> PortFuture<'a, Result<StartKind, PortError>> {
        Box::pin(async move {
            let Some(sandbox) = self.sandbox else {
                return Ok(StartKind::Fresh(FreshReason::NoSaveEver));
            };
            let request = OpenRequest::new(instance, self.web.clone(), turn, seed);
            Ok(sandbox.open(&request).await?.start)
        })
    }

    fn close<'a>(
        &'a self,
        _instance: InstanceId,
        _end: RunEnd,
    ) -> PortFuture<'a, Result<(), PortError>> {
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn given_a_lifecycle_when_the_run_opens_then_it_takes_the_instance_the_turn_reference_and_the_seed_and_gives_the_start()
-> Res {
    let world = World::new()?;
    let driven = Driven {
        sandbox: Some(&world.sandbox),
        web: None,
    };
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
    let driven = Driven {
        sandbox: None,
        web: None,
    };

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
    let driven = Driven {
        sandbox: Some(&world.sandbox),
        web: Some(web.clone()),
    };
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
async fn given_a_run_that_ends_by_failure_or_cancel_when_it_closes_then_the_commands_stop_and_the_box_is_released_and_nothing_is_saved()
-> Res {
    for end in [RunEnd::Failed, RunEnd::Cancelled] {
        let world = World::new()?;
        let sandbox = world.open_box().await?;
        write_files(&world, &sandbox, &[("a.txt", b"unfinished")]).await?;

        close_run(&world.sandbox, &sandbox, &[], end).await?;

        let acts: Vec<&str> = world
            .record
            .calls()
            .iter()
            .filter_map(|call| match call {
                HarnessCall::SandboxSave { .. } => Some("save"),
                HarnessCall::SandboxRelease { .. } => Some("release"),
                _ => None,
            })
            .collect();
        assert_eq!(acts, vec!["release"]);
        assert!(world.sandbox.saved(&sandbox).is_none());
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
