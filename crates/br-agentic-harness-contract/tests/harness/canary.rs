use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{
    BoxId, Command, RunMode, SandboxPort, Seed, SeedEntry, SeedKind,
};

use crate::common::*;
use crate::scenario::*;

pub const CANARY: &[u8] = b"runner-credential-4f9a";

pub fn holds(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|window| window == needle)
}

struct SeedLabels {
    file: &'static str,
    path: &'static str,
    target: &'static str,
}

fn seed_places(seed: &Seed, needle: &[u8], labels: &SeedLabels, places: &mut Vec<&'static str>) {
    for entry in seed.entries() {
        if holds(entry.path.as_str().as_bytes(), needle) {
            places.push(labels.path);
        }
        match &entry.kind {
            SeedKind::File(bytes) if holds(bytes, needle) => places.push(labels.file),
            SeedKind::Symlink(target) if holds(target.as_str().as_bytes(), needle) => {
                places.push(labels.target);
            }
            _ => {}
        }
    }
}

pub fn places_with(world: &World, sandbox: &BoxId, needle: &[u8]) -> Vec<&'static str> {
    let mut places = Vec::new();
    let in_request = SeedLabels {
        file: "open request seed file",
        path: "open request seed path",
        target: "open request seed symlink target",
    };
    for call in world.record.calls() {
        match call {
            HarnessCall::SandboxOpen { request } => {
                seed_places(&request.seed, needle, &in_request, &mut places);
            }
            HarnessCall::SandboxWrite { path, content, .. } => {
                if holds(path.as_str().as_bytes(), needle) {
                    places.push("write path");
                }
                if holds(&content, needle) {
                    places.push("write");
                }
            }
            HarnessCall::SandboxRun { command, .. }
                if holds(command.as_str().as_bytes(), needle) =>
            {
                places.push("command");
            }
            _ => {}
        }
    }
    let files = world.sandbox.files(sandbox).unwrap_or_default();
    if files
        .iter()
        .any(|(path, _)| holds(path.as_str().as_bytes(), needle))
    {
        places.push("file path of the box");
    }
    if files.iter().any(|(_, content)| holds(content, needle)) {
        places.push("file of the box");
    }
    let in_box = SeedLabels {
        file: "seed file of the box",
        path: "seed path of the box",
        target: "seed symlink target of the box",
    };
    seed_places(
        &world.sandbox.seed(sandbox).unwrap_or_default(),
        needle,
        &in_box,
        &mut places,
    );
    places
}

#[tokio::test]
async fn given_a_run_with_a_seed_writes_and_commands_when_the_record_and_the_box_are_searched_for_the_canary_then_it_is_in_none_of_them()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let opened = open_turn(&world, turn, seed_of(&[("skills/s.md", b"a skill")])?).await?;
    write_files(
        &world,
        &opened.box_id,
        &[("a.txt", b"work"), ("b.txt", b"more")],
    )
    .await?;
    write_files(&world, &opened.box_id, &[("a.txt", b"replaced")]).await?;
    world
        .sandbox
        .run(&opened.box_id, &Command::new("ls")?, RunMode::Foreground)
        .await?;

    assert_eq!(
        places_with(&world, &opened.box_id, CANARY),
        Vec::<&str>::new()
    );
    Ok(())
}

#[tokio::test]
async fn given_a_canary_in_a_seed_file_a_write_a_replaced_write_and_a_command_when_the_record_and_the_box_are_searched_then_each_place_is_found()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(None)?;
    let seeded = seed_of(&[("skills/s.md", CANARY)])?;
    let opened = open_turn(&world, turn, seeded).await?;
    write_files(&world, &opened.box_id, &[("env", CANARY)]).await?;
    write_files(&world, &opened.box_id, &[("env", b"clean")]).await?;
    let command = Command::new(format!("echo {}", String::from_utf8_lossy(CANARY)))?;
    world
        .sandbox
        .run(&opened.box_id, &command, RunMode::Foreground)
        .await?;

    let places = places_with(&world, &opened.box_id, CANARY);

    assert_eq!(
        places,
        vec![
            "open request seed file",
            "write",
            "command",
            "seed file of the box",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_canary_in_a_symlink_target_a_seed_path_and_a_write_path_when_the_record_and_the_box_are_searched_then_each_place_is_found()
-> Res {
    let world = World::new()?;
    let canary_text = String::from_utf8_lossy(CANARY).into_owned();
    let seeded = Seed::new(vec![
        SeedEntry::symlink(path("link")?, 0o777, path(&canary_text)?),
        SeedEntry::file(
            path(&format!("dir/{canary_text}"))?,
            0o444,
            b"clean".to_vec(),
        ),
    ])?;
    let opened = open_turn(&world, world.turn_ref(None)?, seeded).await?;
    write_files(
        &world,
        &opened.box_id,
        &[(&format!("out/{canary_text}"), b"clean")],
    )
    .await?;

    let places = places_with(&world, &opened.box_id, CANARY);

    assert_eq!(
        places,
        vec![
            "open request seed path",
            "open request seed symlink target",
            "write path",
            "file path of the box",
            "seed path of the box",
            "seed symlink target of the box",
        ]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_canary_still_in_a_file_of_the_box_when_the_box_is_searched_then_the_file_is_found()
-> Res {
    let world = World::new()?;
    let opened = open_turn(
        &world,
        world.turn_ref(None)?,
        br_agentic_harness_contract::Seed::empty(),
    )
    .await?;
    write_files(&world, &opened.box_id, &[("env", CANARY)]).await?;

    assert_eq!(
        places_with(&world, &opened.box_id, CANARY),
        vec!["write", "file of the box"]
    );
    Ok(())
}

#[tokio::test]
async fn given_the_record_when_a_request_is_opened_then_it_keeps_the_request_in_full_with_the_turn_reference_the_seed_and_the_bytes_of_the_seed_files()
-> Res {
    let world = World::new()?;
    let turn = world.turn_ref(Some(turn_id()?))?;
    let seed = seed_of(&[("s/one.md", b"first bytes"), ("two.md", b"second bytes")])?;
    let request = request_of(turn, seed)?;

    world.sandbox.open(&request).await?;

    let kept: Vec<_> = world
        .record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxOpen { request } => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(kept, vec![request]);
    let bytes: Vec<&[u8]> = kept[0]
        .seed
        .entries()
        .iter()
        .filter_map(|entry| match &entry.kind {
            SeedKind::File(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .collect();
    assert_eq!(bytes, vec![&b"first bytes"[..], &b"second bytes"[..]]);
    Ok(())
}

#[tokio::test]
async fn given_commands_and_writes_when_they_are_made_then_the_record_keeps_each_command_with_its_text_and_each_write_with_its_path_and_bytes_including_a_replaced_file()
-> Res {
    let world = World::new()?;
    let sandbox = world.open_box().await?;
    write_files(&world, &sandbox, &[("a.txt", b"first")]).await?;
    write_files(&world, &sandbox, &[("a.txt", b"second")]).await?;
    for text in ["ls -l", "make test"] {
        world
            .sandbox
            .run(&sandbox, &Command::new(text)?, RunMode::Foreground)
            .await?;
    }

    let mut writes = Vec::new();
    let mut commands = Vec::new();
    for call in world.record.calls() {
        match call {
            HarnessCall::SandboxWrite { path, content, .. } => writes.push((path, content)),
            HarnessCall::SandboxRun { command, .. } => commands.push(command),
            _ => {}
        }
    }
    assert_eq!(
        writes,
        vec![
            (path("a.txt")?, b"first".to_vec()),
            (path("a.txt")?, b"second".to_vec()),
        ]
    );
    assert_eq!(
        commands
            .iter()
            .map(|command| command.as_str())
            .collect::<Vec<_>>(),
        vec!["ls -l", "make test"]
    );
    assert_eq!(world.sandbox.writes(&sandbox), writes);
    Ok(())
}
