use std::time::{Duration, SystemTime};

use br_agentic_harness_contract::{FreshReason, SandboxPort, Seed, StartKind};

use crate::common::*;
use crate::scenario::*;

const WORK: &[(&str, &[u8])] = &[("a.txt", b"kept"), ("dir/b.txt", b"also kept")];

#[tokio::test]
async fn given_a_request_when_the_box_is_opened_then_it_gives_the_box_identity_and_the_start() -> Res
{
    let world = World::new()?;

    let opened = open_turn(&world, world.turn_ref(None)?, Seed::empty()).await?;

    assert!(world.sandbox.files(&opened.box_id).is_some());
    assert_eq!(opened.start, StartKind::Fresh(FreshReason::NoSaveEver));
    Ok(())
}

#[tokio::test]
async fn given_a_request_with_no_parent_turn_when_it_is_opened_then_the_start_is_fresh_with_no_save_ever_and_no_save_is_read()
-> Res {
    let world = World::new()?;
    let (first, _) = saved_turn(&world, None, WORK).await?;
    world.sandbox.make_store_unavailable();

    let opened = open_turn(&world, world.turn_ref(None)?, Seed::empty()).await?;

    assert_eq!(opened.start, StartKind::Fresh(FreshReason::NoSaveEver));
    assert!(
        world
            .sandbox
            .files(&opened.box_id)
            .is_some_and(|files| files.is_empty())
    );
    assert!(world.sandbox.save_of(&first).is_some());
    Ok(())
}

#[tokio::test]
async fn given_a_parent_turn_that_was_never_saved_when_the_box_is_opened_then_the_start_is_fresh_with_no_save_ever()
-> Res {
    let world = World::new()?;
    let never = world.turn_ref(None)?;

    let opened = open_turn(&world, world.turn_ref(Some(never.turn))?, Seed::empty()).await?;

    assert_eq!(opened.start, StartKind::Fresh(FreshReason::NoSaveEver));
    Ok(())
}

#[tokio::test]
async fn given_a_box_saved_and_kept_when_the_next_turn_has_it_as_parent_then_the_start_is_continued_with_the_box_and_the_work_folder_of_the_save()
-> Res {
    let world = World::new()?;
    let (first, kept) = saved_turn(&world, None, WORK).await?;

    let opened = open_turn(&world, world.turn_ref(Some(first.turn))?, Seed::empty()).await?;

    assert_eq!(opened.start, StartKind::Continued);
    assert_eq!(opened.box_id, kept);
    assert!(!world.sandbox.is_released(&opened.box_id));
    assert_eq!(world.sandbox.files(&opened.box_id), Some(content(WORK)?));
    Ok(())
}

#[tokio::test]
async fn given_a_box_changed_after_its_save_when_it_is_continued_then_the_work_folder_is_the_one_of_the_save()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let opened = open_turn(&world, turn, Seed::empty()).await?;
    write_files(&world, &opened.box_id, WORK).await?;
    world.sandbox.save(&opened.box_id).await?;
    write_files(&world, &opened.box_id, &[("late.txt", b"after the save")]).await?;
    world.sandbox.release(&opened.box_id).await?;

    let next = open_turn(&world, world.turn_ref(Some(turn.turn))?, Seed::empty()).await?;

    assert_eq!(next.start, StartKind::Continued);
    assert_eq!(world.sandbox.files(&next.box_id), Some(content(WORK)?));
    Ok(())
}

#[tokio::test]
async fn given_the_keep_of_the_box_ended_when_the_next_turn_opens_then_the_same_open_is_restored_with_the_parent_turn_named()
-> Res {
    let world = World::new()?;
    let (first, kept) = saved_turn(&world, None, WORK).await?;
    world.sandbox.end_keep(&first);

    let opened = open_turn(&world, world.turn_ref(Some(first.turn))?, Seed::empty()).await?;

    assert_eq!(opened.start, StartKind::Restored { turn: first.turn });
    assert_ne!(opened.box_id, kept);
    assert_eq!(world.sandbox.files(&opened.box_id), Some(content(WORK)?));
    Ok(())
}

#[tokio::test]
async fn given_a_box_released_with_no_save_when_the_next_turn_opens_then_the_files_of_that_box_are_not_given()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let opened = open_turn(&world, turn, Seed::empty()).await?;
    write_files(&world, &opened.box_id, WORK).await?;
    world.sandbox.release(&opened.box_id).await?;

    let next = open_turn(&world, world.turn_ref(Some(turn.turn))?, Seed::empty()).await?;

    assert_eq!(next.start, StartKind::Fresh(FreshReason::NoSaveEver));
    assert_ne!(next.box_id, opened.box_id);
    assert!(
        world
            .sandbox
            .files(&next.box_id)
            .is_some_and(|files| files.is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn given_an_expired_save_when_the_next_turn_opens_then_the_start_is_fresh_with_save_expired_also_when_the_box_is_still_alive()
-> Res {
    let world = World::new()?;
    let (first, kept) = saved_turn(&world, None, WORK).await?;
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    world.sandbox.expire_save(&first, Some(when));

    let opened = open_turn(&world, world.turn_ref(Some(first.turn))?, Seed::empty()).await?;

    assert_eq!(
        opened.start,
        StartKind::Fresh(FreshReason::SaveExpired {
            expired_at: Some(when)
        })
    );
    assert_ne!(opened.box_id, kept);
    assert!(
        world
            .sandbox
            .files(&opened.box_id)
            .is_some_and(|files| files.is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn given_a_save_that_expired_at_an_unknown_time_when_the_next_turn_opens_then_save_expired_carries_nothing()
-> Res {
    let world = World::new()?;
    let (first, _) = saved_turn(&world, None, WORK).await?;
    world.sandbox.expire_save(&first, None);

    let opened = open_turn(&world, world.turn_ref(Some(first.turn))?, Seed::empty()).await?;

    assert_eq!(
        opened.start,
        StartKind::Fresh(FreshReason::SaveExpired { expired_at: None })
    );
    Ok(())
}

#[tokio::test]
async fn given_a_save_that_did_not_finish_when_the_next_turn_opens_then_the_start_is_fresh_with_save_failed_naming_the_turn()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    world.sandbox.fail_save(&turn);
    let opened = open_turn(&world, turn, Seed::empty()).await?;
    write_files(&world, &opened.box_id, WORK).await?;
    let saved = world.sandbox.save(&opened.box_id).await;
    world.sandbox.release(&opened.box_id).await?;

    let next = open_turn(&world, world.turn_ref(Some(turn.turn))?, Seed::empty()).await?;

    assert!(matches!(saved, Err(ref error) if !error.is_refusal()));
    assert_eq!(
        next.start,
        StartKind::Fresh(FreshReason::SaveFailed { turn: turn.turn })
    );
    assert!(
        world
            .sandbox
            .files(&next.box_id)
            .is_some_and(|files| files.is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn given_a_save_that_cannot_be_put_in_the_box_when_the_next_turn_opens_then_the_start_is_fresh_with_restore_failed_and_the_box_holds_no_part_of_the_save()
-> Res {
    let world = World::new()?;
    let (first, _) = saved_turn(&world, None, WORK).await?;
    world.sandbox.end_keep(&first);
    world.sandbox.fail_restore(&first);

    let opened = open_turn(
        &world,
        world.turn_ref(Some(first.turn))?,
        seed_of(&[("skills/s.md", b"seed")])?,
    )
    .await?;

    assert_eq!(
        opened.start,
        StartKind::Fresh(FreshReason::RestoreFailed { turn: first.turn })
    );
    assert!(
        world
            .sandbox
            .files(&opened.box_id)
            .is_some_and(|files| files.is_empty())
    );
    assert!(world.sandbox.save_of(&first).is_some());
    Ok(())
}

#[tokio::test]
async fn given_the_place_of_the_saves_is_unavailable_when_a_turn_with_a_parent_opens_then_the_start_is_fresh_with_store_unavailable_and_open_does_not_fail()
-> Res {
    let world = World::new()?;
    let (first, _) = saved_turn(&world, None, WORK).await?;
    world.sandbox.make_store_unavailable();

    let opened = open_turn(&world, world.turn_ref(Some(first.turn))?, Seed::empty()).await?;

    assert_eq!(
        opened.start,
        StartKind::Fresh(FreshReason::StoreUnavailable)
    );
    assert!(
        world
            .sandbox
            .files(&opened.box_id)
            .is_some_and(|files| files.is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn given_the_five_fresh_reasons_when_they_are_made_by_their_scenarios_then_they_are_five_different_values()
-> Res {
    let world = World::new()?;
    let (saved, _) = saved_turn(&world, None, WORK).await?;
    let (expired, _) = saved_turn(&world, None, WORK).await?;
    let (restoring, _) = saved_turn(&world, None, WORK).await?;
    let failed = world.turn_ref(None)?;
    world.sandbox.fail_save(&failed);
    world.sandbox.expire_save(&expired, None);
    world.sandbox.end_keep(&restoring);
    world.sandbox.fail_restore(&restoring);
    let never = world.turn_ref(None)?;
    let mut reasons = vec![
        open_turn(&world, world.turn_ref(Some(never.turn))?, Seed::empty())
            .await?
            .start,
        open_turn(&world, world.turn_ref(Some(expired.turn))?, Seed::empty())
            .await?
            .start,
        open_turn(&world, world.turn_ref(Some(failed.turn))?, Seed::empty())
            .await?
            .start,
        open_turn(&world, world.turn_ref(Some(restoring.turn))?, Seed::empty())
            .await?
            .start,
    ];
    world.sandbox.make_store_unavailable();
    reasons.push(
        open_turn(&world, world.turn_ref(Some(saved.turn))?, Seed::empty())
            .await?
            .start,
    );

    assert!(
        reasons
            .iter()
            .all(|start| matches!(start, StartKind::Fresh(_)))
    );
    for (index, reason) in reasons.iter().enumerate() {
        for other in &reasons[index + 1..] {
            assert_ne!(reason, other);
        }
    }
    Ok(())
}
