use std::sync::Arc;

use br_agentic_contract::memory::FixedToolsPort;
use br_agentic_contract::{AgentLabel, Attachment, DocumentRef, WebAccess};
use br_agentic_harness::{GET_MAIL_FILES, RunEnd, RunLifecycle, SEND_MAIL};
use br_agentic_harness::{Harness, HarnessHistory, MailPort};
use br_agentic_harness_contract::memory::HarnessCall;

use crate::common::*;
use crate::spy::{Seen, spy};

const CREDENTIAL: &str = "runner-credential-7f3a91";

fn writes(world: &World) -> Vec<String> {
    world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxWrite { path, .. } => Some(path.to_string()),
            _ => None,
        })
        .collect()
}

async fn mail_in_thread(
    world: &World,
    names: &[&str],
) -> Result<br_llm_messages::EntryId, Box<dyn std::error::Error>> {
    let mut mail = new_mail(&[world.agent], "files")?;
    for name in names {
        let reference = format!("ref-{name}");
        mail.attachments
            .push(Attachment::new(*name, DocumentRef::new(reference.clone())));
        world
            .mail
            .put_document(DocumentRef::new(reference), b"content".to_vec());
    }
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    store_mail(&world.history, &world.thread, id).await?;
    Ok(id)
}

#[tokio::test]
async fn given_a_runner_whose_prompt_tools_and_mails_hold_its_credential_when_a_whole_run_goes_through_the_harness_then_the_credential_never_reaches_the_box_except_as_the_exact_attachment_that_carries_it()
-> Res {
    let world = World::new()?;
    let seen = Seen::default();
    let secret_tool = runner_tool(&format!("tool_{CREDENTIAL}"))?;
    let harness = Harness::builder(
        Arc::new(world.history.clone()) as Arc<dyn HarnessHistory>,
        Arc::new(FixedPrompt(format!("the key is {CREDENTIAL}"))),
        Arc::new(FixedToolsPort::new(
            &world.contract_record,
            vec![secret_tool],
        )),
    )
    .mail(Arc::new(world.mail.clone()) as Arc<dyn MailPort>)
    .sandbox(spy(&world.sandbox, &seen))
    .web_access(WebAccess::Full)
    .build();
    let name = format!("{CREDENTIAL}.txt");
    let body = format!("body {CREDENTIAL}");
    let mut mail = new_mail(&[world.agent], "files")?;
    mail.attachments
        .push(Attachment::new(&name, DocumentRef::new("ref-secret")));
    world
        .mail
        .put_document(DocumentRef::new("ref-secret"), body.clone().into_bytes());
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    store_mail(&world.history, &world.thread, id).await?;

    harness.open(world.instance).await?;
    harness
        .prompt()
        .prompt(&Doc { n: 0 }, &AgentLabel::new("main")?)
        .await?;
    let tools = offered(&harness, &world).await?;
    let to = agent_id()?;
    call(
        &find(&tools, SEND_MAIL)?,
        &format!(
            "{{\"to\":[\"{}\"],\"subject\":\"{CREDENTIAL}\",\"body\":\"{CREDENTIAL}\"}}",
            to.as_uuid()
        ),
    )
    .await?;
    harness.history().read(&world.thread).await?;
    call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{id}\"}}"),
    )
    .await?;
    harness.close(world.instance, RunEnd::Completed).await?;

    let calls = seen.all();
    let (carriers, others): (Vec<&String>, Vec<&String>) =
        calls.iter().partition(|call| call.starts_with("write "));
    assert_eq!(carriers.len(), 1, "{calls:?}");
    assert!(
        others.iter().all(|call| !call.contains(CREDENTIAL)),
        "{others:?}"
    );
    let kinds: Vec<&str> = others
        .iter()
        .filter_map(|call| call.split(' ').next())
        .collect();
    assert_eq!(kinds, vec!["open", "save", "release"]);
    let expected = format!("write BoxId(\"box-1\") FilePath(\"mail/{id}/{name}\") {body}");
    assert_eq!(carriers[0].as_str(), expected);
    Ok(())
}

#[tokio::test]
async fn given_a_whole_run_when_it_ends_then_only_the_web_access_setting_and_the_attachments_of_a_gated_mail_crossed_into_the_box()
-> Res {
    let world = World::new()?;
    let harness = world.full().web_access(WebAccess::None).build();
    let id = mail_in_thread(&world, &["a.txt"]).await?;
    harness.open(world.instance).await?;
    harness.history().read(&world.thread).await?;
    let tools = offered(&harness, &world).await?;
    call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{id}\"}}"),
    )
    .await?;
    harness.close(world.instance, RunEnd::Completed).await?;

    let kinds: Vec<&str> = world
        .record
        .calls()
        .iter()
        .filter_map(|call| match call {
            HarnessCall::SandboxOpen { .. } => Some("open"),
            HarnessCall::SandboxWrite { .. } => Some("write"),
            HarnessCall::SandboxSave { .. } => Some("save"),
            HarnessCall::SandboxRelease { .. } => Some("release"),
            HarnessCall::SandboxRun { .. }
            | HarnessCall::SandboxRead { .. }
            | HarnessCall::SandboxList { .. }
            | HarnessCall::SandboxKill { .. } => Some("other"),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec!["open", "write", "save", "release"]);
    assert_eq!(writes(&world), vec![format!("mail/{id}/a.txt")]);
    Ok(())
}

#[tokio::test]
async fn given_a_mail_the_gate_passes_when_its_files_are_asked_with_an_open_box_then_its_attachments_are_copied_into_the_work_folder_under_the_mail_id()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let id = mail_in_thread(&world, &["a.txt", "b.txt"]).await?;
    harness.open(world.instance).await?;
    harness.history().read(&world.thread).await?;
    let tools = offered(&harness, &world).await?;

    let result = call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{id}\"}}"),
    )
    .await?;

    assert!(!result.is_error());
    assert_eq!(
        writes(&world),
        vec![format!("mail/{id}/a.txt"), format!("mail/{id}/b.txt")]
    );
    Ok(())
}

#[tokio::test]
async fn given_a_mail_the_gate_refuses_when_its_files_are_asked_with_an_open_box_then_nothing_is_copied_into_the_box()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    mail_in_thread(&world, &["a.txt"]).await?;
    harness.open(world.instance).await?;
    harness.history().read(&world.thread).await?;
    let stranger = new_mail(&[world.agent], "stranger")?;
    let stranger_id = stranger.id.entry_id();
    world.deliver(stranger)?;
    let tools = offered(&harness, &world).await?;

    let result = call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{stranger_id}\"}}"),
    )
    .await?;

    assert!(result.is_error());
    assert!(writes(&world).is_empty());
    Ok(())
}

#[tokio::test]
async fn given_no_open_box_when_the_files_of_a_gated_mail_are_asked_then_the_documents_are_returned_and_nothing_is_written_anywhere()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let id = mail_in_thread(&world, &["a.txt"]).await?;
    harness.history().read(&world.thread).await?;
    let tools = offered(&harness, &world).await?;

    let result = call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{id}\"}}"),
    )
    .await?;

    assert!(!result.is_error());
    assert!(text_of(&result).contains("content"));
    assert!(writes(&world).is_empty());
    Ok(())
}

#[tokio::test]
async fn given_an_attachment_name_that_leaves_its_folder_when_the_files_are_asked_then_it_is_not_copied_and_the_result_says_so()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let id = mail_in_thread(&world, &["../escape.txt", "fine.txt"]).await?;
    harness.open(world.instance).await?;
    harness.history().read(&world.thread).await?;
    let tools = offered(&harness, &world).await?;

    let result = call(
        &find(&tools, GET_MAIL_FILES)?,
        &format!("{{\"mail_id\":\"{id}\"}}"),
    )
    .await?;

    assert_eq!(writes(&world), vec![format!("mail/{id}/fine.txt")]);
    assert!(text_of(&result).contains("not placed in the work folder: \"../escape.txt\""));
    Ok(())
}

#[tokio::test]
async fn given_a_node_with_no_web_access_setting_when_the_run_opens_then_the_harness_does_not_grant_network_access()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    harness.open(world.instance).await?;

    assert!(
        world
            .record
            .calls()
            .iter()
            .all(|call| !matches!(call, HarnessCall::SandboxOpen { web: Some(_), .. }))
    );
    Ok(())
}
