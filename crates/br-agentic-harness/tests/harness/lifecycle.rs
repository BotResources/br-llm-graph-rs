use br_agentic_contract::{Site, WebAccess};
use br_agentic_harness::{ProcessId, RunEnd, RunLifecycle};
use br_agentic_harness_contract::BoxId;
use br_agentic_harness_contract::memory::{HarnessCall, Network};

use crate::common::*;
use crate::doubles::{broken, slow_open};

fn opened_web(world: &World) -> Vec<Option<WebAccess>> {
    world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxOpen { web, .. } => Some(web.clone()),
            _ => None,
        })
        .collect()
}

fn box_of(world: &World) -> Result<BoxId, Box<dyn std::error::Error>> {
    world
        .record
        .calls()
        .iter()
        .find_map(|call| match call {
            HarnessCall::SandboxOpen { .. } => None,
            HarnessCall::SandboxSave { sandbox } => Some(sandbox.clone()),
            _ => None,
        })
        .ok_or_else(|| "no box was saved".into())
}

fn steps(world: &World) -> Vec<&'static str> {
    world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxOpen { .. } => Some("open"),
            HarnessCall::SandboxKill { .. } => Some("kill"),
            HarnessCall::SandboxSave { .. } => Some("save"),
            HarnessCall::SandboxRelease { .. } => Some("release"),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn given_web_access_settings_of_the_node_when_the_run_opens_then_each_goes_to_the_sandbox_port_unchanged()
-> Res {
    let settings = [
        WebAccess::Full,
        WebAccess::None,
        WebAccess::Whitelist(vec![
            Site::new("docs.example.org")?,
            Site::new("api.example.org")?,
        ]),
        WebAccess::Whitelist(Vec::new()),
    ];
    for setting in settings {
        let world = World::new()?;
        let harness = world.full().web_access(setting.clone()).build();

        harness.open(world.instance).await?;

        assert_eq!(opened_web(&world), vec![Some(setting)]);
    }
    Ok(())
}

#[tokio::test]
async fn given_a_node_with_no_web_access_setting_when_the_run_opens_then_none_goes_to_the_sandbox_port_and_the_box_has_no_network()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    harness.open(world.instance).await?;
    harness.close(world.instance, RunEnd::Completed).await?;

    assert_eq!(opened_web(&world), vec![None]);
    let boxed = box_of(&world)?;
    assert_eq!(world.sandbox.network(&boxed), Some(Network::Closed));
    Ok(())
}

#[tokio::test]
async fn given_a_run_with_background_commands_when_it_closes_whatever_the_cause_then_the_commands_stop_then_the_folder_is_saved_then_the_box_is_released()
-> Res {
    for end in [RunEnd::Completed, RunEnd::Failed, RunEnd::Cancelled] {
        let world = World::new()?;
        let harness = world.full().build();
        harness.open(world.instance).await?;
        harness.track_background(world.instance, ProcessId::new("server-one"))?;
        harness.track_background(world.instance, ProcessId::new("server-two"))?;

        harness.close(world.instance, end).await?;

        assert_eq!(
            steps(&world),
            vec!["open", "kill", "kill", "save", "release"]
        );
        let boxed = box_of(&world)?;
        assert!(world.sandbox.is_released(&boxed));
        assert!(world.sandbox.saved(&boxed).is_some());
    }
    Ok(())
}

#[tokio::test]
async fn given_a_process_that_cannot_be_killed_when_the_run_closes_then_the_folder_is_still_saved_and_the_box_released_and_the_failure_is_returned()
-> Res {
    let world = World::new()?;
    let harness = world
        .builder()
        .sandbox(broken(&world.sandbox, true, false))
        .build();
    harness.open(world.instance).await?;
    harness.track_background(world.instance, ProcessId::new("stuck"))?;

    let closed = harness.close(world.instance, RunEnd::Failed).await;

    assert!(closed.is_err());
    assert_eq!(steps(&world), vec!["open", "save", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_a_folder_that_cannot_be_saved_when_the_run_closes_then_the_box_is_still_released_and_the_failure_is_returned()
-> Res {
    let world = World::new()?;
    let harness = world
        .builder()
        .sandbox(broken(&world.sandbox, false, true))
        .build();
    harness.open(world.instance).await?;

    let closed = harness.close(world.instance, RunEnd::Cancelled).await;

    assert!(closed.is_err());
    assert_eq!(steps(&world), vec!["open", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_a_run_with_no_open_box_when_it_closes_then_nothing_is_called_and_nothing_fails()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    harness.close(world.instance, RunEnd::Completed).await?;

    assert!(world.record.calls().is_empty());
    Ok(())
}

#[tokio::test]
async fn given_an_instance_already_open_when_it_opens_again_then_the_harness_refuses_and_opens_no_second_box()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    harness.open(world.instance).await?;

    let again = harness.open(world.instance).await;

    assert!(matches!(again, Err(ref error) if error.is_refusal()));
    assert_eq!(steps(&world), vec!["open"]);
    Ok(())
}

#[tokio::test]
async fn given_two_instances_when_one_closes_then_the_box_of_the_other_stays_open() -> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let other = instance_id()?;
    harness.open(world.instance).await?;
    harness.open(other).await?;

    harness.close(world.instance, RunEnd::Completed).await?;

    assert_eq!(steps(&world), vec!["open", "open", "save", "release"]);
    harness.track_background(other, ProcessId::new("still-here"))?;
    Ok(())
}

#[tokio::test]
async fn given_an_instance_with_no_open_box_when_a_background_process_is_tracked_then_the_harness_refuses()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    let tracked = harness.track_background(world.instance, ProcessId::new("p"));

    assert!(matches!(tracked, Err(ref error) if error.is_refusal()));
    Ok(())
}

#[tokio::test]
async fn given_two_concurrent_opens_of_one_instance_when_both_run_then_one_box_is_opened_one_is_refused_and_close_releases_it()
-> Res {
    let world = World::new()?;
    let harness = world.builder().sandbox(slow_open(&world.sandbox)).build();

    let (first, second) = tokio::join!(harness.open(world.instance), harness.open(world.instance));

    assert_eq!(first.is_ok() as u8 + second.is_ok() as u8, 1);
    assert!(matches!(
        first.err().or(second.err()),
        Some(ref error) if error.is_refusal()
    ));
    assert_eq!(steps(&world), vec!["open"]);
    harness.close(world.instance, RunEnd::Completed).await?;
    assert_eq!(steps(&world), vec!["open", "save", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_an_open_future_dropped_in_flight_when_the_instance_is_opened_again_then_the_open_is_accepted()
-> Res {
    let world = World::new()?;
    let harness = world.builder().sandbox(slow_open(&world.sandbox)).build();

    tokio::select! {
        biased;
        _ = harness.open(world.instance) => {}
        _ = tokio::task::yield_now() => {}
    }
    let again = harness.open(world.instance).await;

    assert!(again.is_ok(), "{again:?}");
    Ok(())
}

#[tokio::test]
async fn given_a_close_while_an_open_is_in_flight_when_the_open_commits_then_the_box_is_saved_and_released_and_the_open_is_refused()
-> Res {
    let world = World::new()?;
    let harness = world.builder().sandbox(slow_open(&world.sandbox)).build();

    let (opened, closed) = tokio::join!(
        harness.open(world.instance),
        harness.close(world.instance, RunEnd::Cancelled)
    );

    assert!(closed.is_ok());
    assert!(
        matches!(opened, Err(ref error) if error.is_refusal()),
        "{opened:?}"
    );
    assert_eq!(steps(&world), vec!["open", "save", "release"]);
    let reopened = harness.open(world.instance).await;
    assert!(reopened.is_ok(), "{reopened:?}");
    Ok(())
}
