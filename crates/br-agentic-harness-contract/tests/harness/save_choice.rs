use br_agentic_harness_contract::{FreshReason, SandboxPort, Seed, StartKind, TurnRef};

use crate::common::*;
use crate::scenario::*;

#[tokio::test]
async fn given_newer_saves_of_another_thread_another_agent_and_another_turn_when_the_next_turn_opens_then_only_the_save_of_the_parent_turn_is_used()
-> Res {
    let world = World::new()?;
    let (parent, _) = saved_turn(&world, None, &[("own.txt", b"parent")]).await?;
    world.sandbox.end_keep(&parent);
    let other_thread = TurnRef::new(thread_ref()?, world.agent, turn_id()?, None);
    let other_agent = TurnRef::new(world.thread, agent_id()?, turn_id()?, None);
    for other in [other_thread, other_agent] {
        let opened = open_turn(&world, other, Seed::empty()).await?;
        write_files(&world, &opened.box_id, &[("foreign.txt", b"other")]).await?;
        world.sandbox.save(&opened.box_id).await?;
        world.sandbox.release(&opened.box_id).await?;
    }
    saved_turn(&world, None, &[("another-turn.txt", b"newer")]).await?;

    let opened = open_turn(&world, world.turn_ref(Some(parent.turn))?, Seed::empty()).await?;

    assert_eq!(opened.start, StartKind::Restored { turn: parent.turn });
    assert_eq!(
        world.sandbox.files(&opened.box_id),
        Some(content(&[("own.txt", b"parent")])?)
    );
    Ok(())
}

#[tokio::test]
async fn given_a_parent_turn_whose_save_is_in_another_thread_or_agent_when_the_box_is_opened_then_the_start_is_fresh_with_no_save_ever()
-> Res {
    let world = World::new()?;
    let (saved, _) = saved_turn(&world, None, &[("a.txt", b"x")]).await?;
    let in_other_thread = TurnRef::new(thread_ref()?, world.agent, turn_id()?, Some(saved.turn));
    let by_other_agent = TurnRef::new(world.thread, agent_id()?, turn_id()?, Some(saved.turn));

    for turn in [in_other_thread, by_other_agent] {
        let opened = open_turn(&world, turn, Seed::empty()).await?;
        assert_eq!(opened.start, StartKind::Fresh(FreshReason::NoSaveEver));
    }
    Ok(())
}

#[tokio::test]
async fn given_three_saved_turns_when_the_next_turn_goes_back_to_the_first_then_its_work_folder_is_the_one_of_the_first_and_the_later_saves_are_not_used()
-> Res {
    let world = World::new()?;
    let (one, _) = saved_turn(&world, None, &[("f.txt", b"one")]).await?;
    let (two, _) = saved_turn(&world, Some(one.turn), &[("f.txt", b"two")]).await?;
    let (three, _) = saved_turn(&world, Some(two.turn), &[("f.txt", b"three")]).await?;
    for turn in [&one, &two, &three] {
        world.sandbox.end_keep(turn);
    }

    let back = open_turn(&world, world.turn_ref(Some(one.turn))?, Seed::empty()).await?;

    assert_eq!(back.start, StartKind::Restored { turn: one.turn });
    assert_eq!(
        world.sandbox.files(&back.box_id),
        Some(content(&[("f.txt", b"one")])?)
    );
    Ok(())
}

#[tokio::test]
async fn given_every_start_when_the_seed_of_the_request_is_given_then_the_box_holds_that_seed_and_not_the_seed_of_an_earlier_turn()
-> Res {
    let world = World::new()?;
    let old_seed = seed_of(&[("skills/old.md", b"old")])?;
    let new_seed = seed_of(&[("skills/new.md", b"new")])?;
    let first = world.turn_ref(None)?;
    let opened = open_turn(&world, first, old_seed.clone()).await?;
    write_files(&world, &opened.box_id, &[("w.txt", b"work")]).await?;
    world.sandbox.save(&opened.box_id).await?;
    world.sandbox.release(&opened.box_id).await?;
    assert_eq!(world.sandbox.seed(&opened.box_id), Some(old_seed));

    let continued = open_turn(&world, world.turn_ref(Some(first.turn))?, new_seed.clone()).await?;
    world.sandbox.release(&continued.box_id).await?;
    world.sandbox.end_keep(&first);
    let restored = open_turn(&world, world.turn_ref(Some(first.turn))?, new_seed.clone()).await?;
    let fresh = open_turn(&world, world.turn_ref(None)?, new_seed.clone()).await?;

    assert_eq!(continued.start, StartKind::Continued);
    assert_eq!(restored.start, StartKind::Restored { turn: first.turn });
    for opened in [continued, restored, fresh] {
        assert_eq!(world.sandbox.seed(&opened.box_id), Some(new_seed.clone()));
    }
    Ok(())
}

#[tokio::test]
async fn given_an_empty_seed_when_the_box_is_opened_then_it_holds_no_seed_file() -> Res {
    let world = World::new()?;

    let opened = open_turn(&world, world.turn_ref(None)?, Seed::empty()).await?;

    assert!(
        world
            .sandbox
            .seed(&opened.box_id)
            .is_some_and(|seed| seed.entries().is_empty())
    );
    Ok(())
}

#[tokio::test]
async fn given_a_seed_when_the_box_is_saved_then_the_save_does_not_keep_the_seed_and_a_fresh_start_holds_the_seed_and_nothing_else()
-> Res {
    let world = World::new()?;
    let seed = seed_of(&[("skills/s.md", b"seed")])?;
    let turn = world.turn_ref(None)?;
    let opened = open_turn(&world, turn, seed.clone()).await?;
    write_files(&world, &opened.box_id, &[("w.txt", b"work")]).await?;
    world.sandbox.save(&opened.box_id).await?;

    let fresh = open_turn(&world, world.turn_ref(None)?, seed.clone()).await?;

    assert_eq!(
        world.sandbox.save_of(&turn),
        Some(content(&[("w.txt", b"work")])?)
    );
    assert_eq!(world.sandbox.seed(&fresh.box_id), Some(seed));
    assert!(
        world
            .sandbox
            .files(&fresh.box_id)
            .is_some_and(|files| files.is_empty())
    );
    Ok(())
}
