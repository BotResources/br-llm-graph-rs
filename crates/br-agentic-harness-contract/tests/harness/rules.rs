use br_agentic_contract::{Attachment, DocumentRef};
use br_agentic_harness_contract::MailPort;
use br_agentic_harness_contract::SandboxPort;

use crate::common::*;
use crate::drivers::{Gate, copy_attachments};

async fn mail_with_file(
    world: &World,
) -> Result<(br_llm_messages::AgentId, br_agentic_contract::MailId), Box<dyn std::error::Error>> {
    let recipient = agent_id()?;
    let mut mail = new_mail(&[recipient], &[], "files")?;
    mail.attachments = vec![Attachment::new("a.txt", DocumentRef::new("doc-a"))];
    let id = mail.id;
    world
        .mail
        .put_document(DocumentRef::new("doc-a"), b"A".to_vec());
    world.mail.send(&world.context()?, mail).await?;
    Ok((recipient, id))
}

#[tokio::test]
async fn given_a_mail_the_gate_has_seen_when_its_attachments_are_copied_then_they_land_in_the_work_folder()
-> Res {
    let world = World::new()?;
    let (recipient, id) = mail_with_file(&world).await?;
    let sandbox = world.open_box().await?;
    let mut gate = Gate::default();
    gate.see(id);

    let copied = copy_attachments(
        &gate,
        &world.mail,
        &world.context_of(recipient)?,
        &world.sandbox,
        &sandbox,
        id,
    )
    .await?;

    assert_eq!(copied, 1);
    assert_eq!(world.sandbox.list(&sandbox, None).await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn given_a_mail_id_the_gate_has_not_seen_when_its_attachments_are_copied_then_it_is_refused_and_nothing_crosses_into_the_box()
-> Res {
    let world = World::new()?;
    let (recipient, id) = mail_with_file(&world).await?;
    let sandbox = world.open_box().await?;
    let mut gate = Gate::default();
    gate.see(mail_id()?);

    let copied = copy_attachments(
        &gate,
        &world.mail,
        &world.context_of(recipient)?,
        &world.sandbox,
        &sandbox,
        id,
    )
    .await;

    assert!(matches!(copied, Err(ref error) if error.is_refusal()));
    assert!(world.sandbox.list(&sandbox, None).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn given_an_empty_gate_when_any_id_is_checked_then_it_fails_closed() -> Res {
    let gate = Gate::default();

    assert!(gate.check(mail_id()?).is_err());
    Ok(())
}
