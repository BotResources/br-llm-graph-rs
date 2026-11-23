use br_agentic_contract::IncomingItem;
use br_agentic_harness_contract::memory::MemorySkills;
use br_agentic_harness_contract::{FilePath, MailPort, SkillName, SkillsPort};

use crate::common::*;

#[tokio::test]
async fn given_two_instances_when_they_use_the_mail_and_skills_ports_at_once_then_each_gets_its_own_complete_answer()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let mail = new_mail(&[recipient], &[], "one")?;
    let id = mail.id;
    world.mail.send(&world.context()?, mail).await?;
    let skills = MemorySkills::new(&world.record).with_skill(
        SkillName::new("pdf")?,
        "work with pdf files",
        vec![(FilePath::new("SKILL.md")?, b"use pdftotext".to_vec())],
    );
    let first = world.context_of(recipient)?;
    let second = world.context_of(recipient)?;
    assert_ne!(first.instance, second.instance);
    let name = SkillName::new("pdf")?;
    let file = FilePath::new("SKILL.md")?;

    let (items_a, items_b, list_a, list_b, read_a, read_b) = tokio::join!(
        world.mail.items(&first, None, limit(5)?),
        world.mail.items(&second, None, limit(5)?),
        skills.list(first.instance),
        skills.list(second.instance),
        skills.read(first.instance, &name, &file),
        skills.read(second.instance, &name, &file),
    );

    for items in [items_a?, items_b?] {
        let ids: Vec<_> = items.items.iter().map(IncomingItem::id).collect();
        assert_eq!(ids, vec![id.entry_id()]);
    }
    for list in [list_a?, list_b?] {
        assert_eq!(list.len(), 1);
    }
    assert_eq!(read_a?, b"use pdftotext");
    assert_eq!(read_b?, b"use pdftotext");
    Ok(())
}
