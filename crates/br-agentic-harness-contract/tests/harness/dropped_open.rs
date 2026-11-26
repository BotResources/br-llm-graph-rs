use br_agentic_harness_contract::{SandboxPort, Seed, StartKind};

use crate::common::*;
use crate::scenario::*;

#[tokio::test]
async fn given_an_open_held_after_its_box_is_made_when_its_future_is_dropped_then_the_box_is_released_and_the_next_open_of_the_same_instance_succeeds()
-> Res {
    let world = World::new()?;
    let request = request_of(world.turn_ref(None)?, Seed::empty())?;
    let hold = world.sandbox.hold_next_open();
    let mut pending = Box::pin(world.sandbox.open(&request));

    let made = tokio::select! {
        _ = &mut pending => return Err("the held open ended".into()),
        made = hold.box_made() => made,
    };
    assert!(!world.sandbox.is_released(&made));
    drop(pending);

    assert!(world.sandbox.is_released(&made));
    let next = world.sandbox.open(&request).await?;
    assert_ne!(next.box_id, made);
    assert!(!world.sandbox.is_released(&next.box_id));
    Ok(())
}

#[tokio::test]
async fn given_an_open_held_after_its_box_is_made_when_the_test_lets_it_proceed_then_the_open_ends_with_the_box_open()
-> Res {
    let world = World::new()?;
    let request = request_of(world.turn_ref(None)?, Seed::empty())?;
    let hold = world.sandbox.hold_next_open();
    let mut pending = Box::pin(world.sandbox.open(&request));

    let made = tokio::select! {
        _ = &mut pending => return Err("the held open ended".into()),
        made = hold.box_made() => made,
    };
    hold.proceed();
    let opened = pending.await?;

    assert_eq!(opened.box_id, made);
    assert!(!world.sandbox.is_released(&made));
    Ok(())
}

#[tokio::test]
async fn given_an_instance_with_an_open_box_when_it_opens_again_then_it_is_refused_until_the_box_is_released()
-> Res {
    let world = World::new()?;
    let request = request_of(world.turn_ref(None)?, Seed::empty())?;
    let first = world.sandbox.open(&request).await?;

    let second = world.sandbox.open(&request).await;
    world.sandbox.release(&first.box_id).await?;
    let third = world.sandbox.open(&request).await;

    assert!(matches!(second, Err(ref error) if error.is_refusal()));
    assert!(third.is_ok());
    Ok(())
}

#[tokio::test]
async fn given_a_continued_open_held_after_its_box_is_made_when_its_future_is_dropped_then_the_keep_is_given_back_and_the_next_open_of_the_parent_is_continued_with_the_same_box()
-> Res {
    let world = World::new()?;
    let (first, kept) = saved_turn(&world, None, &[("work.txt", b"saved work")]).await?;
    let child = world.turn_ref(Some(first.turn))?;
    let request = request_of(child, Seed::empty())?;
    let hold = world.sandbox.hold_next_open();
    let mut pending = Box::pin(world.sandbox.open(&request));

    let made = tokio::select! {
        _ = &mut pending => return Err("the held open ended".into()),
        made = hold.box_made() => made,
    };
    assert_eq!(made, kept);
    assert!(!world.sandbox.is_released(&made));
    drop(pending);

    assert!(world.sandbox.is_released(&made));
    let next = world.sandbox.open(&request).await?;
    assert_eq!(next.start, StartKind::Continued);
    assert_eq!(next.box_id, kept);
    assert_eq!(
        world.sandbox.files(&next.box_id),
        Some(content(&[("work.txt", b"saved work")])?)
    );
    Ok(())
}
