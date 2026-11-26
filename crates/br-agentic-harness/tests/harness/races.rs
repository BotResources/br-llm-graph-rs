use br_agentic_harness::{RunEnd, RunLifecycle};
use br_agentic_harness_contract::OpenFault;

use crate::common::*;
use crate::ledger::{seen_steps, steps};
use crate::ports::{OpenAnswer, Rigged, SaveAnswer};
use crate::spy::Seen;

fn expected_after(end: RunEnd) -> Vec<&'static str> {
    match end {
        RunEnd::Completed => vec!["open", "save", "release"],
        RunEnd::Failed | RunEnd::Cancelled => vec!["open", "release"],
    }
}

#[tokio::test]
async fn given_an_open_whose_future_is_dropped_after_the_port_made_the_box_when_the_instance_closes_and_opens_again_then_the_close_finds_nothing_and_the_open_is_accepted()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let hold = world.sandbox.hold_next_open();

    tokio::select! {
        biased;
        opened = world.open(&harness) => return Err(format!("the open ended: {opened:?}").into()),
        _ = hold.box_made() => {}
    }
    let closed = harness.close(world.instance, RunEnd::Completed).await;
    let again = world.open(&harness).await;

    assert!(closed.is_ok(), "{closed:?}");
    assert!(again.is_ok(), "{again:?}");
    assert_eq!(steps(&world), vec!["open", "open"]);
    harness.close(world.instance, RunEnd::Completed).await?;
    assert_eq!(steps(&world), vec!["open", "open", "save", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_an_open_whose_future_is_dropped_before_the_port_answers_when_the_instance_opens_again_then_the_open_is_accepted_and_no_call_ends_a_box()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let (rig, gate) = Rigged::new(&world.sandbox, &seen).gated();
    let harness = world.builder().sandbox(rig.port()).build();

    tokio::select! {
        biased;
        opened = world.open(&harness) => return Err(format!("the open ended: {opened:?}").into()),
        () = gate.entered() => {}
    }
    harness.close(world.instance, RunEnd::Cancelled).await?;
    gate.go();
    let again = world.open(&harness).await;

    assert!(again.is_ok(), "{again:?}");
    assert_eq!(seen_steps(&seen), vec!["open", "open"]);
    Ok(())
}

#[tokio::test]
async fn given_an_open_future_dropped_in_flight_when_the_instance_is_opened_again_then_the_open_is_accepted()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let harness = world
        .builder()
        .sandbox(Rigged::new(&world.sandbox, &seen).slow_open().port())
        .build();

    tokio::select! {
        biased;
        _ = world.open(&harness) => {}
        _ = tokio::task::yield_now() => {}
    }
    let again = world.open(&harness).await;

    assert!(again.is_ok(), "{again:?}");
    Ok(())
}

#[tokio::test]
async fn given_a_close_while_an_open_is_in_flight_when_the_port_answers_a_box_then_the_close_does_not_fail_and_the_box_is_ended_by_the_end_of_that_close_and_the_open_is_refused()
-> Res {
    for end in [RunEnd::Completed, RunEnd::Failed, RunEnd::Cancelled] {
        let world = World::new()?;
        let harness = world.full().build();
        let hold = world.sandbox.hold_next_open();
        let driver = async {
            let boxed = hold.box_made().await;
            let closed = harness.close(world.instance, end).await;
            let before = steps(&world);
            hold.proceed();
            (boxed, closed, before)
        };

        let (opened, (boxed, closed, before)) = tokio::join!(world.open(&harness), driver);

        assert!(closed.is_ok(), "{closed:?}");
        assert_eq!(before, vec!["open"]);
        assert!(
            matches!(opened, Err(ref error) if error.is_refusal()),
            "{opened:?}"
        );
        assert_eq!(steps(&world), expected_after(end));
        assert!(world.sandbox.is_released(&boxed));
        assert_eq!(
            world.sandbox.saved(&boxed).is_some(),
            end == RunEnd::Completed
        );
        let reopened = world.open(&harness).await;
        assert!(reopened.is_ok(), "{reopened:?}");
    }
    Ok(())
}

#[tokio::test]
async fn given_a_close_while_an_open_is_in_flight_when_the_open_commits_then_the_box_is_saved_and_released_and_the_open_is_refused()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let harness = world
        .builder()
        .sandbox(Rigged::new(&world.sandbox, &seen).slow_open().port())
        .build();

    let (opened, closed) = tokio::join!(
        world.open(&harness),
        harness.close(world.instance, RunEnd::Completed)
    );

    assert!(closed.is_ok());
    assert!(
        matches!(opened, Err(ref error) if error.is_refusal()),
        "{opened:?}"
    );
    assert_eq!(steps(&world), vec!["open", "save", "release"]);
    let reopened = world.open(&harness).await;
    assert!(reopened.is_ok(), "{reopened:?}");
    Ok(())
}

#[tokio::test]
async fn given_two_closes_while_one_open_is_in_flight_when_the_port_answers_a_box_then_the_first_end_decides()
-> Res {
    let cases = [
        (RunEnd::Cancelled, RunEnd::Completed),
        (RunEnd::Failed, RunEnd::Completed),
        (RunEnd::Completed, RunEnd::Failed),
        (RunEnd::Completed, RunEnd::Cancelled),
    ];
    for (first, second) in cases {
        let world = World::new()?;
        let harness = world.full().build();
        let hold = world.sandbox.hold_next_open();
        let driver = async {
            hold.box_made().await;
            let one = harness.close(world.instance, first).await;
            let two = harness.close(world.instance, second).await;
            hold.proceed();
            (one, two)
        };

        let (opened, (one, two)) = tokio::join!(world.open(&harness), driver);

        assert!(one.is_ok() && two.is_ok(), "{one:?} {two:?}");
        assert!(matches!(opened, Err(ref error) if error.is_refusal()));
        assert_eq!(steps(&world), expected_after(first), "{first:?} {second:?}");
    }
    Ok(())
}

#[tokio::test]
async fn given_a_close_while_an_open_is_in_flight_when_the_port_answers_a_fault_then_nothing_is_saved_or_released_and_the_open_returns_the_fault_and_a_later_open_is_not_blocked()
-> Res {
    for end in [RunEnd::Completed, RunEnd::Cancelled] {
        let world = World::new()?;
        let seen = Seen::default();
        let (rig, gate) = Rigged::new(&world.sandbox, &seen)
            .open(OpenAnswer::Image(OpenFault::ImageUnavailable))
            .gated();
        let harness = world.builder().sandbox(rig.port()).build();
        let driver = async {
            gate.entered().await;
            let closed = harness.close(world.instance, end).await;
            gate.go();
            closed
        };

        let (opened, closed) = tokio::join!(world.open(&harness), driver);

        assert!(closed.is_ok(), "{closed:?}");
        let error = opened.err().ok_or("the open must fail")?;
        assert_eq!(OpenFault::of(&error), Some(OpenFault::ImageUnavailable));
        assert_eq!(seen_steps(&seen), vec!["open"]);
        gate.go();
        let later = world
            .open(&harness)
            .await
            .err()
            .ok_or("the open must fail")?;
        assert_eq!(OpenFault::of(&later), Some(OpenFault::ImageUnavailable));
        assert_eq!(seen_steps(&seen), vec!["open", "open"]);
    }
    Ok(())
}

#[tokio::test]
async fn given_a_close_while_an_open_is_in_flight_when_ending_the_box_fails_then_the_open_still_answers_a_refusal_and_every_step_is_tried_and_a_later_open_reaches_the_port()
-> Res {
    let cases = [
        (RunEnd::Completed, SaveAnswer::Fault, false),
        (RunEnd::Completed, SaveAnswer::Works, true),
        (RunEnd::Completed, SaveAnswer::Fault, true),
        (RunEnd::Failed, SaveAnswer::Works, true),
        (RunEnd::Cancelled, SaveAnswer::Works, true),
    ];
    for (end, save, release_fails) in cases {
        let world = World::new()?;
        let seen = Seen::default();
        let mut rig = Rigged::new(&world.sandbox, &seen).save(save);
        if release_fails {
            rig = rig.release_fails();
        }
        let (rig, gate) = rig.gated();
        let harness = world.builder().sandbox(rig.port()).build();
        let driver = async {
            gate.entered().await;
            let closed = harness.close(world.instance, end).await;
            gate.go();
            closed
        };

        let (opened, closed) = tokio::join!(world.open(&harness), driver);

        let label = format!("{end:?} {save:?} {release_fails}");
        assert!(closed.is_ok(), "{label}: {closed:?}");
        assert!(
            matches!(opened, Err(ref error) if error.is_refusal()),
            "{label}: {opened:?}"
        );
        let tried: Vec<&str> = if end == RunEnd::Completed {
            vec!["open", "save", "release"]
        } else {
            vec!["open", "release"]
        };
        assert_eq!(seen_steps(&seen), tried, "{label}");
        gate.go();
        let _ = world.open(&harness).await;
        let mut again = tried;
        again.push("open");
        assert_eq!(seen_steps(&seen), again, "{label}");
    }
    Ok(())
}
