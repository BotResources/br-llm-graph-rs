use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{RunEnd, RunLifecycle, Seed};

use crate::common::*;
use crate::lifecycle_driver::Driven;

#[tokio::test]
async fn given_a_lifecycle_open_held_after_its_box_is_made_when_its_future_is_dropped_then_no_box_of_the_instance_stays_open_and_the_instance_opens_again()
-> Res {
    let world = World::new()?;
    let driven = Driven::with_sandbox(&world.sandbox);
    let instance = instance_id()?;
    let hold = world.sandbox.hold_next_open();
    let mut pending = Box::pin(driven.open(instance, world.turn_ref(None)?, Seed::empty()));

    let made = tokio::select! {
        _ = &mut pending => return Err("the held open ended".into()),
        made = hold.box_made() => made,
    };
    assert!(!world.sandbox.is_released(&made));
    drop(pending);

    assert!(world.sandbox.is_released(&made));
    assert_eq!(driven.box_of(instance), None);
    driven
        .open(instance, world.turn_ref(None)?, Seed::empty())
        .await?;
    let reopened = driven.box_of(instance).ok_or("the instance opens again")?;
    assert_ne!(reopened, made);
    assert!(!world.sandbox.is_released(&reopened));
    Ok(())
}

#[tokio::test]
async fn given_a_close_that_comes_while_the_open_is_in_progress_when_the_open_ends_then_no_box_stays_open_and_a_save_is_made_only_after_a_completed_run()
-> Res {
    for end in [RunEnd::Completed, RunEnd::Failed, RunEnd::Cancelled] {
        let world = World::new()?;
        let driven = Driven::with_sandbox(&world.sandbox);
        let instance = instance_id()?;
        let hold = world.sandbox.hold_next_open();
        let mut pending = Box::pin(driven.open(instance, world.turn_ref(None)?, Seed::empty()));
        let made = tokio::select! {
            _ = &mut pending => return Err("the held open ended".into()),
            made = hold.box_made() => made,
        };

        driven.close(instance, end).await?;
        assert!(!world.sandbox.is_released(&made));
        hold.proceed();
        pending.await?;

        assert!(world.sandbox.is_released(&made));
        assert_eq!(driven.box_of(instance), None);
        let saved = world
            .record
            .calls()
            .iter()
            .any(|call| matches!(call, HarnessCall::SandboxSave { .. }));
        assert_eq!(saved, end == RunEnd::Completed);
        driven
            .open(instance, world.turn_ref(None)?, Seed::empty())
            .await?;
        assert!(driven.box_of(instance).is_some());
    }
    Ok(())
}
