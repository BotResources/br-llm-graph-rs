use br_agentic_contract::{Attachment, DocumentRef};
use br_agentic_harness_contract::memory::HarnessCall;
use br_agentic_harness_contract::{FilePath, MailPort, SandboxPort};

use crate::canary::{CANARY as CREDENTIAL, holds};
use crate::common::*;
use crate::drivers::{Gate, copy_attachments};

#[tokio::test]
async fn given_a_run_that_copies_an_attachment_when_the_writes_are_inspected_then_the_credential_is_in_no_file_of_the_box()
-> Res {
    let world = World::new()?;
    let recipient = agent_id()?;
    let mut mail = new_mail(&[recipient], &[], "files")?;
    mail.attachments = vec![Attachment::new("a.txt", DocumentRef::new("doc-a"))];
    let id = mail.id;
    world
        .mail
        .put_document(DocumentRef::new("doc-a"), b"attachment".to_vec());
    world.mail.send(&world.context()?, mail).await?;
    let sandbox = world.open_box().await?;
    let mut gate = Gate::default();
    gate.see(id);

    copy_attachments(
        &gate,
        &world.mail,
        &world.context_of(recipient)?,
        &world.sandbox,
        &sandbox,
        id,
    )
    .await?;

    let writes = world.sandbox.writes(&sandbox);
    assert_eq!(writes.len(), 1);
    assert!(
        writes
            .iter()
            .all(|(_, content)| !holds(content, CREDENTIAL))
    );
    let files = world.sandbox.files(&sandbox).ok_or("the box is open")?;
    assert!(files.iter().all(|(_, content)| !holds(content, CREDENTIAL)));
    let opened = world
        .record
        .calls()
        .iter()
        .filter(|call| matches!(call, HarnessCall::SandboxOpen { .. }))
        .count();
    assert_eq!(opened, 1);
    Ok(())
}

#[tokio::test]
async fn given_a_credential_written_into_the_box_when_the_writes_are_inspected_then_the_record_shows_it_even_after_the_file_is_replaced()
-> Res {
    let world = World::new()?;
    let sandbox = world.open_box().await?;
    let path = FilePath::new("env")?;
    world
        .sandbox
        .write(&sandbox, &path, CREDENTIAL.to_vec())
        .await?;
    world
        .sandbox
        .write(&sandbox, &path, b"clean".to_vec())
        .await?;

    let writes = world.sandbox.writes(&sandbox);
    let files = world.sandbox.files(&sandbox).ok_or("the box is open")?;

    assert!(writes.iter().any(|(_, content)| holds(content, CREDENTIAL)));
    assert!(files.iter().all(|(_, content)| !holds(content, CREDENTIAL)));
    Ok(())
}
