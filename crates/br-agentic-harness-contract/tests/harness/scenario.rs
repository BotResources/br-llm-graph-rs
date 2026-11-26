use std::error::Error;

use br_agentic_harness_contract::{
    BoxId, FilePath, OpenRequest, Opened, SandboxPort, Seed, SeedEntry, TurnId, TurnRef,
};

use crate::common::*;

pub fn path(text: &str) -> Result<FilePath, Box<dyn Error>> {
    Ok(FilePath::new(text)?)
}

pub fn seed_of(files: &[(&str, &[u8])]) -> Result<Seed, Box<dyn Error>> {
    let mut entries = Vec::new();
    for (name, bytes) in files {
        entries.push(SeedEntry::file(path(name)?, 0o444, bytes.to_vec()));
    }
    Ok(Seed::new(entries)?)
}

pub fn request_of(turn: TurnRef, seed: Seed) -> Result<OpenRequest, Box<dyn Error>> {
    Ok(OpenRequest::new(instance_id()?, None, turn, seed))
}

pub async fn open_turn(world: &World, turn: TurnRef, seed: Seed) -> Result<Opened, Box<dyn Error>> {
    let request = request_of(turn, seed)?;
    Ok(world.sandbox.open(&request).await?)
}

pub async fn write_files(
    world: &World,
    sandbox: &BoxId,
    files: &[(&str, &[u8])],
) -> Result<(), Box<dyn Error>> {
    for (name, bytes) in files {
        world
            .sandbox
            .write(sandbox, &path(name)?, bytes.to_vec())
            .await?;
    }
    Ok(())
}

pub async fn saved_turn(
    world: &World,
    parent: Option<TurnId>,
    files: &[(&str, &[u8])],
) -> Result<(TurnRef, BoxId), Box<dyn Error>> {
    let turn = world.turn_ref(parent)?;
    let opened = open_turn(world, turn, Seed::empty()).await?;
    write_files(world, &opened.box_id, files).await?;
    world.sandbox.save(&opened.box_id).await?;
    world.sandbox.release(&opened.box_id).await?;
    Ok((turn, opened.box_id))
}

pub type Listed = Vec<(FilePath, Vec<u8>)>;

pub fn content(files: &[(&str, &[u8])]) -> Result<Listed, Box<dyn Error>> {
    let mut listed = Vec::new();
    for (name, bytes) in files {
        listed.push((path(name)?, bytes.to_vec()));
    }
    listed.sort();
    Ok(listed)
}
