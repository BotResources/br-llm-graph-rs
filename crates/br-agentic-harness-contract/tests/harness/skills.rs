use br_agentic_harness_contract::memory::MemorySkills;
use br_agentic_harness_contract::{FilePath, SkillName, SkillsPort};

use crate::common::*;

fn name(text: &str) -> Result<SkillName, Box<dyn std::error::Error>> {
    Ok(SkillName::new(text)?)
}

fn path(text: &str) -> Result<FilePath, Box<dyn std::error::Error>> {
    Ok(FilePath::new(text)?)
}

fn skills(world: &World) -> Result<MemorySkills, Box<dyn std::error::Error>> {
    Ok(MemorySkills::new(&world.record)
        .with_skill(
            name("pdf")?,
            "work with pdf files",
            vec![(path("SKILL.md")?, b"use pdftotext".to_vec())],
        )
        .with_skill(name("csv")?, "work with csv files", Vec::new()))
}

#[tokio::test]
async fn given_two_skills_when_they_are_listed_then_each_has_its_name_and_a_short_description()
-> Res {
    let world = World::new()?;

    let listed = skills(&world)?.list(instance_id()?).await?;

    let pairs: Vec<_> = listed
        .iter()
        .map(|skill| (skill.name.as_str(), skill.description.as_str()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("csv", "work with csv files"),
            ("pdf", "work with pdf files")
        ]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_skill_file_when_it_is_read_then_the_port_gives_the_bytes() -> Res {
    let world = World::new()?;

    let bytes = skills(&world)?
        .read(instance_id()?, &name("pdf")?, &path("SKILL.md")?)
        .await?;

    assert_eq!(bytes, b"use pdftotext");
    Ok(())
}

#[tokio::test]
async fn given_a_skill_or_a_path_that_does_not_exist_when_it_is_read_then_it_is_refused_with_a_reason()
-> Res {
    let world = World::new()?;
    let port = skills(&world)?;

    let no_skill = port
        .read(instance_id()?, &name("missing")?, &path("SKILL.md")?)
        .await;
    let no_path = port
        .read(instance_id()?, &name("pdf")?, &path("other.md")?)
        .await;

    assert!(matches!(no_skill, Err(ref error) if error.is_refusal()));
    assert!(matches!(no_path, Err(ref error) if error.is_refusal()));
    Ok(())
}
