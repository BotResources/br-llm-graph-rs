use br_agentic_harness::{GET_MAIL_FILES, RunEnd, RunLifecycle, SEND_MAIL};
use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{FilePath, FreshReason, Seed, SeedEntry, StartKind, TurnRef};

use crate::common::*;

#[tokio::test]
async fn given_a_runner_with_no_mail_sandbox_or_skills_port_when_the_harness_is_used_then_it_does_the_history_work_and_adds_no_tool_and_does_not_fail()
-> Res {
    let world = World::new()?;
    let harness = world.builder_with(vec![runner_tool("lookup")?]).build();

    let tools = offered(&harness, &world).await?;
    harness.open_and_close(&world).await?;

    let names: Vec<&str> = tools.iter().map(|tool| tool.name().as_str()).collect();
    assert_eq!(names, vec!["lookup"]);
    assert!(harness.incoming().is_none());
    assert!(world.record.calls().is_empty());
    Ok(())
}

#[tokio::test]
async fn given_a_runner_with_a_skills_port_when_the_tools_and_the_prompt_are_built_then_the_harness_adds_no_tool_and_no_prompt_text_and_never_reads_the_skills()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    let tools = offered(&harness, &world).await?;
    let prompt = harness
        .prompt()
        .prompt(
            &Doc { n: 0 },
            &br_agentic_contract::AgentLabel::new("main")?,
        )
        .await?;

    let mut names: Vec<&str> = tools.iter().map(|tool| tool.name().as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, vec![GET_MAIL_FILES, SEND_MAIL]);
    assert_eq!(prompt, "the prompt of the runner");
    assert!(harness.skills().is_some());
    assert!(!world.record.calls().iter().any(|call| matches!(
        call,
        HarnessCall::SkillsList { .. } | HarnessCall::SkillsRead { .. }
    )));
    Ok(())
}

#[tokio::test]
async fn given_a_runner_with_a_mail_port_only_when_the_run_opens_and_closes_then_nothing_fails_and_no_sandbox_call_is_made()
-> Res {
    let world = World::new()?;
    let harness = world
        .builder()
        .mail(std::sync::Arc::new(world.mail.clone()))
        .build();

    world.open(&harness).await?;
    harness.close(world.instance, RunEnd::Failed).await?;

    assert!(world.record.calls().is_empty());
    assert!(harness.incoming().is_some());
    Ok(())
}

#[tokio::test]
async fn given_a_runner_with_no_sandbox_port_when_the_run_opens_with_a_turn_and_a_seed_then_the_start_is_fresh_with_no_save_ever_and_no_call_is_made()
-> Res {
    let world = World::new()?;
    let harness = world
        .builder()
        .mail(std::sync::Arc::new(world.mail.clone()))
        .build();
    let turn = TurnRef::new(world.thread, world.agent, turn_id()?, Some(turn_id()?));
    let seed = Seed::new(vec![SeedEntry::file(
        FilePath::new("notes.txt")?,
        0o644,
        b"notes".to_vec(),
    )])?;

    let start = harness.open(world.instance, turn, seed).await?;
    harness.close(world.instance, RunEnd::Completed).await?;

    assert_eq!(start, StartKind::Fresh(FreshReason::NoSaveEver));
    assert!(world.record.calls().is_empty());
    Ok(())
}

trait OpenAndClose {
    async fn open_and_close(&self, world: &World) -> Res;
}

impl OpenAndClose for br_agentic_harness::Harness<Doc> {
    async fn open_and_close(&self, world: &World) -> Res {
        world.open(self).await?;
        self.close(world.instance, RunEnd::Completed).await?;
        Ok(())
    }
}
