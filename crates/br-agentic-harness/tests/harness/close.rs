use br_agentic_harness::{ProcessId, RunEnd, RunLifecycle, SandboxPort};
use br_agentic_harness_contract::{AlreadySaved, FilePath, FreshReason, Seed, StartKind, TurnRef};

use crate::common::*;
use crate::ledger::{released_box, seen_steps, steps};
use crate::ports::{KILL_FAILURE, Rigged, SAVE_FAILURE, SaveAnswer};
use crate::spy::Seen;

const ENDS: [RunEnd; 3] = [RunEnd::Completed, RunEnd::Failed, RunEnd::Cancelled];

async fn run_with_two_servers(world: &World, harness: &br_agentic_harness::Harness<Doc>) -> Res {
    world.open(harness).await?;
    harness.track_background(world.instance, ProcessId::new("server-one"))?;
    harness.track_background(world.instance, ProcessId::new("server-two"))?;
    Ok(())
}

#[tokio::test]
async fn given_a_run_with_background_commands_when_it_closes_completed_then_the_commands_stop_then_the_folder_is_saved_then_the_box_is_released()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    run_with_two_servers(&world, &harness).await?;

    harness.close(world.instance, RunEnd::Completed).await?;

    assert_eq!(
        steps(&world),
        vec!["open", "kill", "kill", "save", "release"]
    );
    let boxed = released_box(&world)?;
    assert!(world.sandbox.is_released(&boxed));
    assert!(world.sandbox.saved(&boxed).is_some());
    assert!(world.sandbox.running(&boxed).is_empty());
    Ok(())
}

#[tokio::test]
async fn given_a_run_with_background_commands_when_it_closes_failed_or_cancelled_then_the_commands_stop_and_the_box_is_released_and_the_folder_is_never_saved()
-> Res {
    for end in [RunEnd::Failed, RunEnd::Cancelled] {
        let world = World::new()?;
        let harness = world.full().build();
        run_with_two_servers(&world, &harness).await?;
        harness.close(world.instance, end).await?;
        let boxed = released_box(&world)?;

        assert_eq!(steps(&world), vec!["open", "kill", "kill", "release"]);
        assert!(world.sandbox.is_released(&boxed));
        assert!(world.sandbox.saved(&boxed).is_none());
        assert!(world.sandbox.save_of(&world.turn_ref).is_none());
        assert!(world.sandbox.running(&boxed).is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn given_work_in_the_folder_when_a_run_closes_then_the_work_is_saved_only_after_completed_and_only_then_does_the_next_turn_restore_it()
-> Res {
    for (end, kept) in [
        (RunEnd::Completed, true),
        (RunEnd::Failed, false),
        (RunEnd::Cancelled, false),
    ] {
        let world = World::new()?;
        let seen = Seen::default();
        let rig = Rigged::new(&world.sandbox, &seen);
        let harness = world.builder().sandbox(rig.clone().port()).build();
        world.open(&harness).await?;
        let boxed = rig.boxes().pop().ok_or("no box was opened")?;
        let work = FilePath::new("work.txt")?;
        rig.write(&boxed, &work, b"done".to_vec()).await?;

        harness.close(world.instance, end).await?;

        let saved = world.sandbox.save_of(&world.turn_ref);
        assert_eq!(saved, kept.then(|| vec![(work.clone(), b"done".to_vec())]));
        assert_eq!(seen_steps(&seen).contains(&"save".to_owned()), kept);
        world.sandbox.end_keep(&world.turn_ref);
        let next = TurnRef::new(
            world.thread,
            world.agent,
            turn_id()?,
            Some(world.turn_ref.turn),
        );
        let start = harness.open(world.instance, next, Seed::empty()).await?;
        let expected = if kept {
            StartKind::Restored {
                turn: world.turn_ref.turn,
            }
        } else {
            StartKind::Fresh(FreshReason::NoSaveEver)
        };
        assert_eq!(start, expected);
    }
    Ok(())
}

#[tokio::test]
async fn given_a_process_that_cannot_be_killed_when_the_run_closes_then_the_save_of_a_completed_run_and_the_release_still_happen_and_the_failure_is_returned()
-> Res {
    for (end, expected) in [
        (RunEnd::Completed, vec!["open", "kill", "save", "release"]),
        (RunEnd::Failed, vec!["open", "kill", "release"]),
        (RunEnd::Cancelled, vec!["open", "kill", "release"]),
    ] {
        let world = World::new()?;
        let seen = Seen::default();
        let harness = world
            .builder()
            .sandbox(Rigged::new(&world.sandbox, &seen).kill_fails().port())
            .build();
        world.open(&harness).await?;
        harness.track_background(world.instance, ProcessId::new("stuck"))?;

        let closed = harness.close(world.instance, end).await;

        let error = closed.err().ok_or("the close must fail")?;
        assert!(format!("{error:?}").contains(KILL_FAILURE), "{error:?}");
        assert_eq!(seen_steps(&seen), expected);
    }
    Ok(())
}

#[tokio::test]
async fn given_a_folder_that_cannot_be_saved_when_a_completed_run_closes_then_the_box_is_still_released_and_the_failure_is_returned()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let harness = world
        .builder()
        .sandbox(
            Rigged::new(&world.sandbox, &seen)
                .save(SaveAnswer::Fault)
                .port(),
        )
        .build();
    world.open(&harness).await?;

    let closed = harness.close(world.instance, RunEnd::Completed).await;

    let error = closed.err().ok_or("the close must fail")?;
    assert!(format!("{error:?}").contains(SAVE_FAILURE), "{error:?}");
    assert_eq!(seen_steps(&seen), vec!["open", "save", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_a_save_refused_because_the_turn_has_one_already_when_a_completed_run_closes_then_the_save_is_taken_as_done_and_the_box_is_released()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let harness = world
        .builder()
        .sandbox(
            Rigged::new(&world.sandbox, &seen)
                .save(SaveAnswer::AlreadySaved)
                .port(),
        )
        .build();
    world.open(&harness).await?;
    harness.track_background(world.instance, ProcessId::new("p"))?;

    let closed = harness.close(world.instance, RunEnd::Completed).await;

    assert!(closed.is_ok(), "{closed:?}");
    assert_eq!(seen_steps(&seen), vec!["open", "kill", "save", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_a_turn_that_closes_twice_when_the_second_save_is_refused_as_already_made_then_the_first_save_stays_and_the_close_succeeds()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let rig = Rigged::new(&world.sandbox, &seen);
    let harness = world.builder().sandbox(rig.clone().port()).build();
    for (index, text) in ["first", "second"].into_iter().enumerate() {
        world.open(&harness).await?;
        let boxed = rig.boxes().pop().ok_or("no box was opened")?;
        let file = FilePath::new(&format!("{text}.txt"))?;
        rig.write(&boxed, &file, text.as_bytes().to_vec()).await?;

        let closed = harness.close(world.instance, RunEnd::Completed).await;

        assert!(closed.is_ok(), "{index}: {closed:?}");
    }

    let kept = world.sandbox.save_of(&world.turn_ref).ok_or("no save")?;
    assert_eq!(kept, vec![(FilePath::new("first.txt")?, b"first".to_vec())]);
    assert_eq!(
        seen_steps(&seen),
        vec!["open", "save", "release", "open", "save", "release"]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_save_refused_for_another_reason_or_a_near_miss_of_the_already_saved_reason_when_a_completed_run_closes_then_the_refusal_is_a_failure_of_close_and_the_box_is_released()
-> Res {
    for answer in [SaveAnswer::OtherRefusal, SaveAnswer::NearMissRefusal] {
        let world = World::new()?;
        let seen = Seen::default();
        let harness = world
            .builder()
            .sandbox(Rigged::new(&world.sandbox, &seen).save(answer).port())
            .build();
        world.open(&harness).await?;

        let closed = harness.close(world.instance, RunEnd::Completed).await;

        let error = closed.err().ok_or("the close must fail")?;
        assert!(error.is_refusal(), "{error:?}");
        assert!(!AlreadySaved::is(&error));
        assert_eq!(seen_steps(&seen), vec!["open", "save", "release"]);
    }
    Ok(())
}

#[tokio::test]
async fn given_a_kill_that_fails_and_a_save_that_fails_when_a_completed_run_closes_then_the_first_failure_is_returned()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let harness = world
        .builder()
        .sandbox(
            Rigged::new(&world.sandbox, &seen)
                .kill_fails()
                .save(SaveAnswer::Fault)
                .port(),
        )
        .build();
    world.open(&harness).await?;
    harness.track_background(world.instance, ProcessId::new("stuck"))?;

    let closed = harness.close(world.instance, RunEnd::Completed).await;

    let error = closed.err().ok_or("the close must fail")?;
    assert!(format!("{error:?}").contains(KILL_FAILURE), "{error:?}");
    assert!(!format!("{error:?}").contains(SAVE_FAILURE), "{error:?}");
    assert_eq!(seen_steps(&seen), vec!["open", "kill", "save", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_a_run_with_no_open_box_when_it_closes_then_nothing_is_called_and_nothing_fails()
-> Res {
    for end in ENDS {
        let world = World::new()?;
        let harness = world.full().build();

        harness.close(world.instance, end).await?;

        assert!(world.record.calls().is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn given_two_instances_when_one_closes_then_the_box_of_the_other_stays_open() -> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let other = instance_id()?;
    world.open(&harness).await?;
    harness.open(other, world.turn_ref, Seed::empty()).await?;

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
