use std::time::{Duration, SystemTime};

use br_agentic_harness_contract::{AlreadySaved, SandboxPort, Seed, TurnRef};

use crate::common::*;
use crate::scenario::*;

#[tokio::test]
async fn given_a_work_folder_when_it_is_saved_then_the_receipt_names_the_turn_the_time_and_the_raw_size_in_bytes()
-> Res {
    let world = World::new()?;
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(42);
    world.sandbox.set_clock(at);
    let turn = world.turn_ref(None)?;
    let opened = open_turn(&world, turn, seed_of(&[("s.md", b"not counted")])?).await?;
    write_files(
        &world,
        &opened.box_id,
        &[("a.txt", b"12345"), ("b/c.txt", b"678")],
    )
    .await?;

    let receipt = world.sandbox.save(&opened.box_id).await?;

    assert_eq!(receipt.turn, turn.turn);
    assert_eq!(receipt.made_at, at);
    assert_eq!(receipt.bytes, 8);
    Ok(())
}

#[tokio::test]
async fn given_saves_that_differ_in_the_thread_only_the_agent_only_or_the_turn_only_when_they_are_made_then_they_are_three_more_saves_beside_the_first()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let variants = [
        TurnRef::new(thread_ref()?, turn.agent, turn.turn, None),
        TurnRef::new(turn.thread, agent_id()?, turn.turn, None),
        TurnRef::new(turn.thread, turn.agent, turn_id()?, None),
    ];

    let first = open_turn(&world, turn, Seed::empty()).await?;
    write_files(&world, &first.box_id, &[("a.txt", b"first")]).await?;
    world.sandbox.save(&first.box_id).await?;
    for variant in variants {
        let opened = open_turn(&world, variant, Seed::empty()).await?;
        write_files(&world, &opened.box_id, &[("a.txt", b"variant")]).await?;
        world.sandbox.save(&opened.box_id).await?;
    }

    assert_eq!(
        world.sandbox.save_of(&turn),
        Some(content(&[("a.txt", b"first")])?)
    );
    for variant in variants {
        assert_eq!(
            world.sandbox.save_of(&variant),
            Some(content(&[("a.txt", b"variant")])?)
        );
    }
    Ok(())
}

#[tokio::test]
async fn given_a_turn_that_already_has_a_save_when_it_is_saved_again_then_it_is_refused_the_first_save_stays_and_a_typed_test_tells_this_refusal_from_others()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let first = open_turn(&world, turn, Seed::empty()).await?;
    write_files(&world, &first.box_id, &[("a.txt", b"first")]).await?;
    world.sandbox.save(&first.box_id).await?;
    world.sandbox.release(&first.box_id).await?;
    let second = open_turn(&world, turn, Seed::empty()).await?;
    write_files(&world, &second.box_id, &[("a.txt", b"second")]).await?;

    let again = world.sandbox.save(&second.box_id).await;
    let other = world
        .sandbox
        .read(&second.box_id, &path("missing.txt")?)
        .await;

    let Err(error) = again else {
        return Err("a second save of the same turn is refused".into());
    };
    assert!(error.is_refusal());
    assert!(AlreadySaved::is(&error));
    let Err(other) = other else {
        return Err("a missing file is refused".into());
    };
    assert!(other.is_refusal());
    assert!(!AlreadySaved::is(&other));
    assert!(!AlreadySaved::is(
        &br_agentic_harness_contract::PortError::fault("x")
    ));
    assert_eq!(
        world.sandbox.save_of(&turn),
        Some(content(&[("a.txt", b"first")])?)
    );
    Ok(())
}

#[tokio::test]
async fn given_a_box_that_was_saved_by_another_box_of_the_same_turn_when_it_is_released_then_it_is_not_the_box_that_is_continued()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let first = open_turn(&world, turn, Seed::empty()).await?;
    world.sandbox.save(&first.box_id).await?;
    world.sandbox.release(&first.box_id).await?;
    let second = open_turn(&world, turn, Seed::empty()).await?;
    let _ = world.sandbox.save(&second.box_id).await;
    world.sandbox.release(&second.box_id).await?;

    let next = open_turn(&world, world.turn_ref(Some(turn.turn))?, Seed::empty()).await?;

    assert_eq!(next.box_id, first.box_id);
    Ok(())
}
