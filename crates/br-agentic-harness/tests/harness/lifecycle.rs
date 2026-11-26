use br_agentic_contract::{Site, WebAccess};
use br_agentic_harness::RunLifecycle;
use br_agentic_harness_contract::memory::Network;
use br_agentic_harness_contract::{OpenFault, PortError, Seed, StartKind};

use crate::common::*;
use crate::ledger::{released_box, requests, steps};
use crate::ports::{OpenAnswer, Rigged};
use crate::spy::Seen;

fn every_start_kind() -> Result<Vec<StartKind>, Box<dyn std::error::Error>> {
    use br_agentic_harness_contract::FreshReason;
    use std::time::{Duration, SystemTime};
    let turn = turn_id()?;
    let expired = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    Ok(vec![
        StartKind::Continued,
        StartKind::Restored { turn },
        StartKind::Fresh(FreshReason::NoSaveEver),
        StartKind::Fresh(FreshReason::SaveExpired {
            expired_at: Some(expired),
        }),
        StartKind::Fresh(FreshReason::SaveExpired { expired_at: None }),
        StartKind::Fresh(FreshReason::SaveFailed { turn }),
        StartKind::Fresh(FreshReason::RestoreFailed { turn }),
        StartKind::Fresh(FreshReason::StoreUnavailable),
    ])
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

        world.open(&harness).await?;

        let sent: Vec<Option<WebAccess>> = requests(&world)
            .into_iter()
            .map(|request| request.web_access)
            .collect();
        assert_eq!(sent, vec![Some(setting)]);
    }
    Ok(())
}

#[tokio::test]
async fn given_a_node_with_no_web_access_setting_when_the_run_opens_then_none_goes_to_the_sandbox_port_and_the_box_has_no_network()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    world.open(&harness).await?;
    harness
        .close(world.instance, br_agentic_harness::RunEnd::Completed)
        .await?;

    let sent: Vec<Option<WebAccess>> = requests(&world)
        .into_iter()
        .map(|request| request.web_access)
        .collect();
    assert_eq!(sent, vec![None]);
    let boxed = released_box(&world)?;
    assert_eq!(world.sandbox.network(&boxed), Some(Network::Closed));
    Ok(())
}

#[tokio::test]
async fn given_a_sandbox_port_that_answers_each_start_kind_when_the_run_opens_then_the_runner_gets_that_start_kind_unchanged()
-> Res {
    for start in every_start_kind()? {
        let world = World::new()?;
        let seen = Seen::default();
        let port = Rigged::new(&world.sandbox, &seen)
            .open(OpenAnswer::Start(start))
            .port();
        let harness = world.builder().sandbox(port).build();

        let answered = world.open(&harness).await?;

        assert_eq!(answered, start);
    }
    Ok(())
}

#[tokio::test]
async fn given_a_saved_turn_when_the_next_turn_opens_then_the_runner_gets_the_start_that_the_real_sandbox_decides()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let first = world.open(&harness).await?;
    harness
        .close(world.instance, br_agentic_harness::RunEnd::Completed)
        .await?;
    world.sandbox.end_keep(&world.turn_ref);
    let next = br_agentic_harness_contract::TurnRef::new(
        world.thread,
        world.agent,
        turn_id()?,
        Some(world.turn_ref.turn),
    );

    let second = harness.open(world.instance, next, Seed::empty()).await?;

    assert_eq!(
        first,
        StartKind::Fresh(br_agentic_harness_contract::FreshReason::NoSaveEver)
    );
    assert_eq!(
        second,
        StartKind::Restored {
            turn: world.turn_ref.turn
        }
    );
    Ok(())
}

#[tokio::test]
async fn given_a_sandbox_port_that_answers_an_open_fault_when_the_run_opens_then_the_same_fault_comes_back_with_its_kind_and_no_run_is_kept()
-> Res {
    for fault in [OpenFault::ImageUnavailable, OpenFault::ImageFailing] {
        let world = World::new()?;
        let harness = world.full().build();
        world.sandbox.fail_next_open(fault);

        let failed = world.open(&harness).await;

        let error = failed.err().ok_or("the open must fail")?;
        assert_eq!(OpenFault::of(&error), Some(fault));
        assert!(!error.is_refusal());
        harness
            .close(world.instance, br_agentic_harness::RunEnd::Completed)
            .await?;
        assert_eq!(steps(&world), vec!["open"]);
        let again = world.open(&harness).await;
        assert!(again.is_ok(), "{again:?}");
    }
    Ok(())
}

#[tokio::test]
async fn given_a_sandbox_port_that_answers_a_plain_fault_or_a_refusal_when_the_run_opens_then_the_kind_of_the_answer_is_not_changed()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let fault = world
        .builder()
        .sandbox(
            Rigged::new(&world.sandbox, &seen)
                .open(OpenAnswer::PlainFault)
                .port(),
        )
        .build();
    let refusal = world
        .builder()
        .sandbox(
            Rigged::new(&world.sandbox, &seen)
                .open(OpenAnswer::Refusal)
                .port(),
        )
        .build();

    let faulted = world.open(&fault).await.err().ok_or("must fail")?;
    let refused = world.open(&refusal).await.err().ok_or("must fail")?;

    assert!(matches!(faulted, PortError::Fault(_)));
    assert_eq!(OpenFault::of(&faulted), None);
    assert_eq!(
        refused.reason().map(|reason| reason.as_str()),
        Some("the provider refuses")
    );
    Ok(())
}

#[tokio::test]
async fn given_an_instance_already_open_when_it_opens_again_then_the_harness_refuses_and_opens_no_second_box()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    world.open(&harness).await?;

    let again = world.open(&harness).await;

    assert!(matches!(again, Err(ref error) if error.is_refusal()));
    assert_eq!(steps(&world), vec!["open"]);
    Ok(())
}

#[tokio::test]
async fn given_two_concurrent_opens_of_one_instance_when_both_run_then_one_box_is_opened_one_is_refused_and_close_releases_it()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let harness = world
        .builder()
        .sandbox(Rigged::new(&world.sandbox, &seen).slow_open().port())
        .build();

    let (first, second) = tokio::join!(world.open(&harness), world.open(&harness));

    assert_eq!(first.is_ok() as u8 + second.is_ok() as u8, 1);
    assert!(matches!(
        first.err().or(second.err()),
        Some(ref error) if error.is_refusal()
    ));
    assert_eq!(steps(&world), vec!["open"]);
    harness
        .close(world.instance, br_agentic_harness::RunEnd::Completed)
        .await?;
    assert_eq!(steps(&world), vec!["open", "save", "release"]);
    Ok(())
}
