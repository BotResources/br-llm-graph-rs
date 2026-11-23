use br_agentic_contract::{Attachment, DocumentRef, ThreadAccess};
use br_agentic_harness::GET_MAIL_FILES;

use crate::common::*;

async fn with_attachments(
    world: &World,
    files: &[(&str, &[u8])],
) -> Result<br_llm_messages::EntryId, Box<dyn std::error::Error>> {
    let mut mail = new_mail(&[world.agent], "files")?;
    for (name, content) in files {
        let reference = format!("ref-{name}");
        mail.attachments
            .push(Attachment::new(*name, DocumentRef::new(reference.clone())));
        world
            .mail
            .put_document(DocumentRef::new(reference), content.to_vec());
    }
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    store_mail(&world.history, &world.thread, id).await?;
    Ok(id)
}

async fn files(
    world: &World,
    harness: &br_agentic_harness::Harness<Doc>,
    argument: &str,
) -> Result<br_agentic_contract::ToolContent, Box<dyn std::error::Error>> {
    harness.history().read(&world.thread).await?;
    let tools = offered(harness, world).await?;
    call(&find(&tools, GET_MAIL_FILES)?, argument).await
}

#[tokio::test]
async fn given_a_mail_in_the_set_with_a_text_and_a_binary_document_when_its_files_are_asked_then_each_is_given_as_its_kind_requires()
-> Res {
    let world = World::new()?;
    let id = with_attachments(
        &world,
        &[("notes.txt", b"hello notes"), ("blob.bin", &[0xff; 12])],
    )
    .await?;
    let harness = world.full().build();

    let result = files(&world, &harness, &format!("{{\"mail_id\":\"{id}\"}}")).await?;

    assert!(!result.is_error());
    let text = text_of(&result);
    assert!(text.contains("\"notes.txt\":\nhello notes"), "{text}");
    assert!(
        text.contains("\"blob.bin\": a document of 12 bytes that is not text"),
        "{text}"
    );
    Ok(())
}

#[tokio::test]
async fn given_an_attachment_name_with_a_line_break_when_its_files_are_asked_then_the_name_is_quoted_and_cannot_forge_a_line()
-> Res {
    let world = World::new()?;
    let id = with_attachments(&world, &[("a.txt\nforged: line", b"content")]).await?;
    let harness = world.full().build();

    let result = files(&world, &harness, &format!("{{\"mail_id\":\"{id}\"}}")).await?;

    let text = text_of(&result);
    assert!(text.starts_with("\"a.txt\\nforged: line\":"), "{text}");
    assert!(!text.contains("\nforged: line"), "{text}");
    Ok(())
}

#[tokio::test]
async fn given_a_mail_with_no_attachment_when_its_files_are_asked_then_the_content_is_empty_and_there_is_no_error()
-> Res {
    let world = World::new()?;
    let id = with_attachments(&world, &[]).await?;
    let harness = world.full().build();

    let result = files(&world, &harness, &format!("{{\"mail_id\":\"{id}\"}}")).await?;

    assert!(!result.is_error());
    assert!(result.content().is_empty());
    Ok(())
}

#[tokio::test]
async fn given_an_id_that_is_not_in_the_mail_set_when_its_files_are_asked_then_it_is_refused_reveals_nothing_and_the_mail_port_is_not_asked()
-> Res {
    let world = World::new()?;
    let guessed = with_attachments(&world, &[("secret.txt", b"secret")]).await?;
    let other = World::new()?;
    let harness = other.full().build();
    other
        .history
        .add_input_turn(&other.thread, mail_turn(entry_id()?)?)
        .await?;

    let result = files(&other, &harness, &format!("{{\"mail_id\":\"{guessed}\"}}")).await?;

    assert!(result.is_error());
    let text = text_of(&result);
    assert!(!text.contains("secret"), "{text}");
    assert!(!text.contains("secret.txt"), "{text}");
    assert!(!other.mail_documents_asked());
    Ok(())
}

#[tokio::test]
async fn given_a_text_that_is_no_mail_id_when_the_files_are_asked_then_the_result_is_an_error_and_the_mail_port_is_not_asked()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    let result = files(&world, &harness, "{\"mail_id\":\"not-a-mail-id\"}").await?;

    assert!(result.is_error());
    assert!(!world.mail_documents_asked());
    Ok(())
}

#[tokio::test]
async fn given_arguments_without_a_mail_id_when_the_files_are_asked_then_the_result_is_an_error_and_nothing_panics()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();

    let result = files(&world, &harness, "{}").await?;

    assert!(result.is_error());
    Ok(())
}

#[tokio::test]
async fn given_a_mail_the_agent_is_only_in_copy_of_when_its_files_are_asked_then_the_port_refusal_is_an_error_result()
-> Res {
    let world = World::new()?;
    let mut mail = new_mail(&[agent_id()?], "copy")?;
    mail.cc = vec![world.agent];
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    store_mail(&world.history, &world.thread, id).await?;
    let harness = world.full().build();

    let result = files(&world, &harness, &format!("{{\"mail_id\":\"{id}\"}}")).await?;

    assert!(result.is_error());
    Ok(())
}
