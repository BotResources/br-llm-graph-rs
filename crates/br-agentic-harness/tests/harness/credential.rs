use std::error::Error;
use std::sync::Arc;

use br_agentic_contract::memory::FixedToolsPort;
use br_agentic_contract::{AgentLabel, Attachment, DocumentRef, WebAccess};
use br_agentic_harness::RunLifecycle;
use br_agentic_harness::{GET_MAIL_FILES, Harness, HarnessHistory, MailPort, RunEnd, SEND_MAIL};
use br_agentic_harness_contract::memory::{HarnessCall, MemorySkills};
use br_agentic_harness_contract::{FilePath, Seed, SeedEntry, SkillName};

use crate::common::*;
use crate::doubles::runner_tool_described;
use crate::ports::Rigged;
use crate::spy::Seen;

const CANARY: &str = "runner-credential-canary-5c91e2";

fn holds(haystack: &[u8], needle: &str) -> bool {
    let needle = needle.as_bytes();
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn attachment_mail(
    world: &World,
    subject: &str,
    name: &str,
    document: &str,
    body: &str,
) -> Result<br_llm_messages::EntryId, Box<dyn Error>> {
    let mut mail = new_mail(&[world.agent], subject)?;
    mail.attachments
        .push(Attachment::new(name, DocumentRef::new(document)));
    world
        .mail
        .put_document(DocumentRef::new(document), body.as_bytes().to_vec());
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    Ok(id)
}

async fn canary_reached_the_box(
    end: RunEnd,
    attachment_body: &str,
) -> Result<bool, Box<dyn Error>> {
    let world = World::new()?;
    let seen = Seen::default();
    let rig = Rigged::new(&world.sandbox, &seen);
    let skills = MemorySkills::new(&world.record).with_skill(
        SkillName::new(format!("skill-{CANARY}"))?,
        format!("uses {CANARY}"),
        vec![(FilePath::new("SKILL.md")?, CANARY.as_bytes().to_vec())],
    );
    let harness = Harness::builder(
        Arc::new(world.history.clone()) as Arc<dyn HarnessHistory>,
        Arc::new(FixedPrompt(format!("the key is {CANARY}"))),
        Arc::new(FixedToolsPort::new(
            &world.contract_record,
            vec![runner_tool_described(
                &format!("tool_{CANARY}"),
                &format!("a tool that uses {CANARY}"),
            )?],
        )),
    )
    .mail(Arc::new(world.mail.clone()) as Arc<dyn MailPort>)
    .skills(Arc::new(skills))
    .sandbox(rig.clone().port())
    .web_access(WebAccess::Full)
    .build();
    let gated = attachment_mail(
        &world,
        &format!("subject {CANARY}"),
        "report.txt",
        "ref-report",
        attachment_body,
    )?;
    store_mail(&world.history, &world.thread, gated).await?;
    let stranger = attachment_mail(
        &world,
        &format!("stranger {CANARY}"),
        &format!("{CANARY}.txt"),
        "ref-stranger",
        CANARY,
    )?;
    let seed = Seed::new(vec![SeedEntry::file(
        FilePath::new("notes.txt")?,
        0o644,
        b"plain notes".to_vec(),
    )])?;

    harness.open(world.instance, world.turn_ref, seed).await?;
    harness
        .prompt()
        .prompt(&Doc { n: 0 }, &AgentLabel::new("main")?)
        .await?;
    let tools = offered(&harness, &world).await?;
    let recipient = agent_id()?;
    call(
        &find(&tools, SEND_MAIL)?,
        &format!(
            "{{\"to\":[\"{}\"],\"subject\":\"{CANARY}\",\"body\":\"{CANARY}\"}}",
            recipient.as_uuid()
        ),
    )
    .await?;
    harness.history().read(&world.thread).await?;
    call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{gated}\"}}"),
    )
    .await?;
    let refused = call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{stranger}\"}}"),
    )
    .await?;
    assert!(refused.is_error());
    harness.close(world.instance, end).await?;

    let in_calls = world.record.calls().iter().any(|recorded| match recorded {
        HarnessCall::SandboxWrite { content, .. } => holds(content, CANARY),
        HarnessCall::SandboxOpen { .. }
        | HarnessCall::SandboxRun { .. }
        | HarnessCall::SandboxRead { .. }
        | HarnessCall::SandboxList { .. }
        | HarnessCall::SandboxKill { .. }
        | HarnessCall::SandboxSave { .. }
        | HarnessCall::SandboxRelease { .. } => format!("{recorded:?}").contains(CANARY),
        _ => false,
    });
    let in_seen = seen.all().iter().any(|line| line.contains(CANARY));
    let in_boxes = rig.boxes().iter().any(|boxed| {
        let files = world.sandbox.files(boxed).unwrap_or_default();
        files
            .iter()
            .any(|(path, content)| holds(content, CANARY) || path.as_str().contains(CANARY))
            || world
                .sandbox
                .seed(boxed)
                .is_some_and(|seed| format!("{seed:?}").contains(CANARY))
    });
    let in_save = world
        .sandbox
        .save_of(&world.turn_ref)
        .unwrap_or_default()
        .iter()
        .any(|(path, content)| holds(content, CANARY) || path.as_str().contains(CANARY));
    Ok(in_calls || in_seen || in_boxes || in_save)
}

#[tokio::test]
async fn given_a_canary_credential_in_the_prompt_a_tool_name_a_mail_a_skill_and_an_attachment_the_gate_refuses_when_a_whole_run_ends_whatever_the_cause_then_no_call_file_save_or_seed_of_the_box_holds_it()
-> Res {
    for end in [RunEnd::Completed, RunEnd::Failed, RunEnd::Cancelled] {
        let reached = canary_reached_the_box(end, "a plain report").await?;

        assert!(!reached, "{end:?}");
    }
    Ok(())
}

#[tokio::test]
async fn given_the_same_run_when_the_gated_attachment_itself_carries_the_canary_then_the_search_finds_it_in_the_box_so_the_search_is_not_blind()
-> Res {
    for end in [RunEnd::Completed, RunEnd::Failed, RunEnd::Cancelled] {
        let reached = canary_reached_the_box(end, CANARY).await?;

        assert!(reached, "{end:?}");
    }
    Ok(())
}
