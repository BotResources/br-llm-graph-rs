use br_agentic_contract::IncomingItem;
use br_agentic_harness::{GET_MAIL_FILES, RunLifecycle};
use br_llm_messages::TurnEntry;

use crate::common::*;

fn files_arguments(id: br_llm_messages::EntryId) -> String {
    format!("{{\"mail_id\":\"{id}\"}}")
}

async fn files_result(
    harness: &br_agentic_harness::Harness<Doc>,
    world: &World,
    id: br_llm_messages::EntryId,
) -> Result<br_agentic_contract::ToolContent, Box<dyn std::error::Error>> {
    let tools = offered(harness, world).await?;
    call(&find(&tools, GET_MAIL_FILES)?, &files_arguments(id)).await
}

fn deliver_to_agent(world: &World, id: br_llm_messages::EntryId) -> Res {
    let mut mail = new_mail(&[world.agent], "work")?;
    mail.id = br_agentic_contract::MailId::new(id);
    world.deliver(mail)
}

#[tokio::test]
async fn given_a_mail_in_the_thread_when_the_history_is_read_through_the_harness_then_get_mail_files_accepts_its_id()
-> Res {
    let world = World::new()?;
    let id = entry_id()?;
    store_mail(&world.history, &world.thread, id).await?;
    deliver_to_agent(&world, id)?;
    let harness = world.full().build();

    harness.history().read(&world.thread).await?;

    assert!(!files_result(&harness, &world, id).await?.is_error());
    Ok(())
}

#[tokio::test]
async fn given_a_mail_written_through_the_harness_history_when_its_files_are_asked_then_the_id_is_accepted()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let id = entry_id()?;
    deliver_to_agent(&world, id)?;

    store_mail(harness.history().as_ref(), &world.thread, id).await?;

    assert!(!files_result(&harness, &world, id).await?.is_error());
    Ok(())
}

#[tokio::test]
async fn given_a_mail_written_as_a_turn_entry_when_its_files_are_asked_then_the_id_is_accepted()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let id = entry_id()?;
    deliver_to_agent(&world, id)?;

    harness
        .history()
        .add_entry(
            &world.thread,
            world.turn()?,
            TurnEntry::from(mail_entry(id)?),
        )
        .await?;

    assert!(!files_result(&harness, &world, id).await?.is_error());
    Ok(())
}

#[tokio::test]
async fn given_a_mail_that_only_the_mail_port_lists_when_its_files_are_asked_then_the_id_is_refused_because_the_set_is_filled_from_the_history_only()
-> Res {
    let world = World::new()?;
    let id = entry_id()?;
    deliver_to_agent(&world, id)?;
    let harness = world.full().build();
    let listed = harness
        .incoming()
        .ok_or("no incoming port")?
        .items(&world.context(), None, limit(10)?)
        .await?;
    assert!(
        listed
            .items
            .iter()
            .any(|item| matches!(item, IncomingItem::Mail(m) if m.id().entry_id() == id))
    );

    let result = files_result(&harness, &world, id).await?;

    assert!(result.is_error());
    assert!(!world.mail_documents_asked());
    Ok(())
}

#[tokio::test]
async fn given_a_write_the_runner_history_refuses_when_the_mail_id_is_asked_then_it_is_refused()
-> Res {
    let world = World::new()?;
    let harness = world.full().build();
    let other = thread_ref()?;
    let id = entry_id()?;
    deliver_to_agent(&world, id)?;

    let written = harness
        .history()
        .add_input_turn(&other, mail_turn(id)?)
        .await;

    assert!(written.is_err());
    assert!(files_result(&harness, &world, id).await?.is_error());
    Ok(())
}

#[tokio::test]
async fn given_a_mail_seen_on_one_thread_when_another_thread_asks_for_its_files_then_it_is_refused()
-> Res {
    let world = World::new()?;
    let id = entry_id()?;
    store_mail(&world.history, &world.thread, id).await?;
    deliver_to_agent(&world, id)?;
    let harness = world.full().build();
    harness.history().read(&world.thread).await?;
    let other = br_agentic_contract::RunContext::new(
        world.instance,
        thread_ref()?,
        world.agent,
        world.author.clone(),
        world.conversation,
    );
    let tools = harness.tools().tools(&Doc { n: 0 }, &other).await?;

    let result = call(&find(&tools, GET_MAIL_FILES)?, &files_arguments(id)).await?;

    assert!(result.is_error());
    Ok(())
}

#[tokio::test]
async fn given_a_conversation_with_no_mail_when_any_id_is_asked_then_it_fails_closed() -> Res {
    let world = World::new()?;
    let harness = world.full().build();
    harness.history().read(&world.thread).await?;

    let result = files_result(&harness, &world, entry_id()?).await?;

    assert!(result.is_error());
    Ok(())
}

#[tokio::test]
async fn given_a_run_that_saw_a_mail_when_it_closes_then_the_mail_set_of_its_thread_is_empty() -> Res
{
    let world = World::new()?;
    let harness = world.full().build();
    let mail = new_mail(&[world.agent], "work")?;
    let id = mail.id.entry_id();
    world.deliver(mail)?;
    store_mail(harness.history().as_ref(), &world.thread, id).await?;
    let tools = offered(&harness, &world).await?;
    let files = find(&tools, GET_MAIL_FILES)?;
    assert!(!call(&files, &files_arguments(id)).await?.is_error());

    harness
        .close(world.instance, br_agentic_harness::RunEnd::Completed)
        .await?;

    let after = call(&files, &files_arguments(id)).await?;
    assert!(after.is_error());
    Ok(())
}
