use br_agentic_harness_contract::{
    FilePath, InvalidFilePath, InvalidSeed, Seed, SeedEntry, SeedKind,
};

use crate::common::*;
use crate::scenario::path;

const EMPTY_HASH: &str = "b160bdab7f3325d003f6a687247f34365a8c24a8224dfa16d3ce2bf6119032bb";
const MIXED_HASH: &str = "e9d493ec36c1a9d5821d5c53991dddb733630e54f3caec12b14613593a8f90df";

fn mixed() -> Result<Vec<SeedEntry>, Box<dyn std::error::Error>> {
    Ok(vec![
        SeedEntry::file(
            path("skills/pdf/SKILL.md")?,
            0o444,
            b"use pdftotext".to_vec(),
        ),
        SeedEntry::directory(path("skills")?, 0o555),
        SeedEntry::symlink(path("skills/latest")?, 0o777, path("pdf")?),
    ])
}

fn hash_of(entries: Vec<SeedEntry>) -> Result<String, Box<dyn std::error::Error>> {
    Ok(Seed::new(entries)?.content_hash().to_hex())
}

#[test]
fn given_the_three_kinds_of_entry_when_a_seed_is_made_then_each_has_a_path_a_mode_and_a_kind() -> Res
{
    let seed = Seed::new(mixed()?)?;

    let kinds: Vec<_> = seed
        .entries()
        .iter()
        .map(|entry| {
            (
                entry.path.as_str().to_owned(),
                entry.mode,
                entry.kind.clone(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("skills".to_owned(), 0o555, SeedKind::Directory),
            (
                "skills/latest".to_owned(),
                0o777,
                SeedKind::Symlink(path("pdf")?)
            ),
            (
                "skills/pdf/SKILL.md".to_owned(),
                0o444,
                SeedKind::File(b"use pdftotext".to_vec())
            ),
        ]
    );
    Ok(())
}

#[test]
fn given_a_path_or_a_target_that_leaves_the_seed_root_when_it_is_built_then_it_is_not_a_path_of_an_entry()
 {
    for bad in ["/etc/passwd", "../up", "a/../../up", "a\\b"] {
        assert!(FilePath::new(bad).is_err(), "{bad}");
    }
    assert!(matches!(
        FilePath::new("/abs"),
        Err(InvalidFilePath::Absolute(_))
    ));
    assert!(matches!(
        FilePath::new("a/.."),
        Err(InvalidFilePath::Escapes(_))
    ));
    assert!(matches!(
        FilePath::new("a\\b"),
        Err(InvalidFilePath::Character(_))
    ));
}

#[test]
fn given_two_entries_with_the_same_path_when_a_seed_is_made_then_it_is_refused_also_when_the_paths_differ_only_by_spelling()
-> Res {
    let same = Seed::new(vec![
        SeedEntry::file(path("a.txt")?, 0o444, b"one".to_vec()),
        SeedEntry::directory(path("a.txt")?, 0o555),
    ]);
    let spelled = Seed::new(vec![
        SeedEntry::file(path("a/b.txt")?, 0o444, Vec::new()),
        SeedEntry::file(path("a//./b.txt")?, 0o444, Vec::new()),
    ]);

    assert!(
        matches!(same, Err(InvalidSeed::DuplicatePath(ref duplicate)) if duplicate.as_str() == "a.txt")
    );
    assert!(matches!(spelled, Err(InvalidSeed::DuplicatePath(_))));
    Ok(())
}

#[test]
fn given_an_empty_seed_when_it_is_made_then_it_is_valid_and_holds_no_entry() -> Res {
    let made = Seed::new(Vec::new())?;

    assert!(made.is_empty());
    assert_eq!(made, Seed::empty());
    assert_eq!(Seed::default(), Seed::empty());
    Ok(())
}

#[test]
fn given_the_same_entries_in_another_order_when_the_seeds_are_made_then_the_hash_is_the_same() -> Res
{
    let mut reversed = mixed()?;
    reversed.reverse();

    assert_eq!(hash_of(mixed()?)?, hash_of(reversed)?);
    assert_eq!(Seed::new(mixed()?)?, {
        let mut other = mixed()?;
        other.rotate_left(1);
        Seed::new(other)?
    });
    Ok(())
}

#[test]
fn given_a_change_of_a_path_a_mode_a_kind_a_target_or_one_byte_when_the_hash_is_made_then_it_changes()
-> Res {
    let base = hash_of(mixed()?)?;
    let file =
        |name: &str, mode: u32, bytes: &[u8]| -> Result<SeedEntry, Box<dyn std::error::Error>> {
            Ok(SeedEntry::file(path(name)?, mode, bytes.to_vec()))
        };
    let one = hash_of(vec![file("a", 0o444, b"x")?])?;

    let changed = [
        hash_of(vec![file("b", 0o444, b"x")?])?,
        hash_of(vec![file("a", 0o555, b"x")?])?,
        hash_of(vec![file("a", 0o444, b"y")?])?,
        hash_of(vec![file("a", 0o444, b"xx")?])?,
        hash_of(vec![SeedEntry::directory(path("a")?, 0o444)])?,
        hash_of(vec![SeedEntry::symlink(path("a")?, 0o444, path("x")?)])?,
        hash_of(vec![SeedEntry::symlink(path("a")?, 0o444, path("y")?)])?,
        hash_of(Vec::new())?,
    ];
    let mut every = vec![one, base];
    every.extend(changed);
    for (index, hash) in every.iter().enumerate() {
        for other in &every[index + 1..] {
            assert_ne!(hash, other);
        }
    }
    Ok(())
}

#[test]
fn given_the_seeds_of_the_frozen_vectors_when_the_hash_is_made_then_it_is_the_value_that_every_process_and_release_of_the_minor_gives()
-> Res {
    assert_eq!(hash_of(Vec::new())?, EMPTY_HASH);
    assert_eq!(hash_of(mixed()?)?, MIXED_HASH);
    let hash = Seed::new(mixed()?)?.content_hash();
    let from_bytes: String = hash
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(from_bytes, MIXED_HASH);
    assert_eq!(hash.to_string(), MIXED_HASH);
    assert_eq!(hash.to_hex(), MIXED_HASH);
    Ok(())
}
