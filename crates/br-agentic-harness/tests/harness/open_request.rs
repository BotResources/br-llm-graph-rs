use br_agentic_contract::{Site, WebAccess};
use br_agentic_harness::{RunEnd, RunLifecycle};
use br_agentic_harness_contract::{FilePath, OpenRequest, Seed, SeedEntry, TurnRef};

use crate::common::*;
use crate::ledger::{released_box, requests};

fn path(text: &str) -> Result<FilePath, Box<dyn std::error::Error>> {
    Ok(FilePath::new(text)?)
}

fn rich_seed() -> Result<Seed, Box<dyn std::error::Error>> {
    Ok(Seed::new(vec![
        SeedEntry::file(path("src/main.rs")?, 0o644, b"fn main() {}\n".to_vec()),
        SeedEntry::directory(path("src")?, 0o755),
        SeedEntry::symlink(path("link")?, 0o777, path("src/main.rs")?),
        SeedEntry::file(path("empty")?, 0o600, Vec::new()),
        SeedEntry::file(path("binary.bin")?, 0o755, vec![0, 255, 10, 13, 0, 128]),
        SeedEntry::file(path("big.dat")?, 0o644, vec![7; 70_000]),
    ])?)
}

#[tokio::test]
async fn given_an_instance_a_web_access_setting_a_turn_reference_and_a_seed_when_the_run_opens_then_the_sandbox_port_gets_exactly_one_open_request_built_from_them()
-> Res {
    let world = World::new()?;
    let web = WebAccess::Whitelist(vec![Site::new("docs.example.org")?]);
    let harness = world.full().web_access(web.clone()).build();
    let turn = TurnRef::new(world.thread, world.agent, turn_id()?, Some(turn_id()?));
    let seed = rich_seed()?;

    harness.open(world.instance, turn, seed.clone()).await?;

    assert_eq!(
        requests(&world),
        vec![OpenRequest::new(world.instance, Some(web), turn, seed)]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_seed_when_the_run_opens_then_the_sandbox_port_receives_every_entry_in_its_order_with_no_part_added_dropped_or_changed()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let seed = rich_seed()?;
    let given_entries = seed.entries().to_vec();
    let given_hash = seed.content_hash();

    harness
        .open(world.instance, world.turn_ref, seed.clone())
        .await?;

    let received = requests(&world)
        .into_iter()
        .next()
        .ok_or("no open request")?
        .seed;
    assert_eq!(received.entries(), given_entries.as_slice());
    assert_eq!(received.content_hash(), given_hash);
    assert_eq!(received, seed);
    Ok(())
}

#[tokio::test]
async fn given_the_empty_seed_and_a_turn_without_parent_when_the_run_opens_then_the_request_holds_the_empty_seed_and_no_parent()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    world.open(&harness).await?;

    let request = requests(&world)
        .into_iter()
        .next()
        .ok_or("no open request")?;
    assert!(request.seed.is_empty());
    assert_eq!(request.turn.parent, None);
    assert_eq!(request.turn, world.turn_ref);
    Ok(())
}

#[tokio::test]
async fn given_a_seed_when_the_run_opens_then_the_box_that_the_port_made_holds_the_seed_that_the_runner_gave()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let seed = rich_seed()?;
    harness
        .open(world.instance, world.turn_ref, seed.clone())
        .await?;
    harness.close(world.instance, RunEnd::Failed).await?;

    let boxed = released_box(&world)?;

    assert_eq!(world.sandbox.seed(&boxed), Some(seed));
    Ok(())
}
