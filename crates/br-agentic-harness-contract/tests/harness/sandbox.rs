use br_agentic_harness_contract::memory::{HarnessCall, Network};
use br_agentic_harness_contract::{
    Command, CommandResult, FilePath, RunMode, RunOutcome, SandboxPort, WebAccess,
};

use crate::common::*;

fn path(text: &str) -> Result<FilePath, Box<dyn std::error::Error>> {
    Ok(FilePath::new(text)?)
}

fn command(text: &str) -> Result<Command, Box<dyn std::error::Error>> {
    Ok(Command::new(text)?)
}

#[tokio::test]
async fn given_a_foreground_command_when_it_runs_then_its_result_comes_back() -> Res {
    let world = World::new()?;
    world.sandbox.script(
        "echo hi",
        CommandResult {
            exit_code: 0,
            stdout: b"hi\n".to_vec(),
            stderr: Vec::new(),
        },
    );
    let sandbox = world.sandbox.open(instance_id()?, None).await?;

    let outcome = world
        .sandbox
        .run(&sandbox, &command("echo hi")?, RunMode::Foreground)
        .await?;

    assert!(matches!(outcome, RunOutcome::Finished(result) if result.stdout == b"hi\n"));
    Ok(())
}

#[tokio::test]
async fn given_a_background_command_when_it_runs_then_it_returns_a_process_identity_at_once_and_keeps_running()
-> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;

    let outcome = world
        .sandbox
        .run(&sandbox, &command("sleep 1000")?, RunMode::Background)
        .await?;

    let RunOutcome::Started(process) = outcome else {
        return Err("a process identity".into());
    };
    assert_eq!(world.sandbox.running(&sandbox), vec![process]);
    Ok(())
}

#[tokio::test]
async fn given_a_file_when_it_is_written_then_it_is_read_back_replaced_and_listed_under_a_path()
-> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;
    world
        .sandbox
        .write(&sandbox, &path("work/a.txt")?, b"one".to_vec())
        .await?;
    world
        .sandbox
        .write(&sandbox, &path("work/a.txt")?, b"two".to_vec())
        .await?;
    world
        .sandbox
        .write(&sandbox, &path("other/b.txt")?, b"b".to_vec())
        .await?;

    let read = world.sandbox.read(&sandbox, &path("work/a.txt")?).await?;
    let under_work = world.sandbox.list(&sandbox, Some(&path("work")?)).await?;
    let everything = world.sandbox.list(&sandbox, None).await?;

    assert_eq!(read, b"two");
    assert_eq!(under_work, vec![path("work/a.txt")?]);
    assert_eq!(everything.len(), 2);
    Ok(())
}

#[tokio::test]
async fn given_a_file_that_does_not_exist_when_it_is_read_then_it_is_a_refusal_with_a_reason() -> Res
{
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;

    let read = world.sandbox.read(&sandbox, &path("missing.txt")?).await;

    assert!(matches!(read, Err(ref error) if error.is_refusal()));
    Ok(())
}

#[tokio::test]
async fn given_a_background_process_when_it_is_killed_then_it_stops_and_killing_an_ended_process_changes_nothing()
-> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;
    let RunOutcome::Started(process) = world
        .sandbox
        .run(&sandbox, &command("server")?, RunMode::Background)
        .await?
    else {
        return Err("a process identity".into());
    };

    world.sandbox.kill(&sandbox, &process).await?;
    world.sandbox.kill(&sandbox, &process).await?;

    assert!(world.sandbox.running(&sandbox).is_empty());
    Ok(())
}

#[tokio::test]
async fn given_a_work_folder_when_it_is_saved_then_the_provider_holds_a_copy_of_the_files_at_that_moment()
-> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;
    world
        .sandbox
        .write(&sandbox, &path("a.txt")?, b"kept".to_vec())
        .await?;

    world.sandbox.save(&sandbox).await?;
    world
        .sandbox
        .write(&sandbox, &path("a.txt")?, b"later".to_vec())
        .await?;

    assert_eq!(
        world.sandbox.saved(&sandbox),
        Some(vec![(path("a.txt")?, b"kept".to_vec())])
    );
    Ok(())
}

#[tokio::test]
async fn given_a_released_box_when_any_operation_is_called_then_it_is_a_fault_and_releasing_again_is_accepted()
-> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;
    world.sandbox.release(&sandbox).await?;

    world.sandbox.release(&sandbox).await?;
    let save = world.sandbox.save(&sandbox).await;
    let write = world
        .sandbox
        .write(&sandbox, &path("a.txt")?, Vec::new())
        .await;

    assert!(matches!(save, Err(ref error) if !error.is_refusal()));
    assert!(matches!(write, Err(ref error) if !error.is_refusal()));
    assert!(world.sandbox.is_released(&sandbox));
    Ok(())
}

#[tokio::test]
async fn given_a_box_that_was_never_opened_when_it_is_used_then_it_is_a_fault() -> Res {
    let world = World::new()?;
    let other = world.sandbox.open(instance_id()?, None).await?;
    let unknown = br_agentic_harness_contract::BoxId::new("elsewhere");

    let read = world.sandbox.read(&unknown, &path("a.txt")?).await;

    assert!(matches!(read, Err(ref error) if !error.is_refusal()));
    assert_ne!(other, unknown);
    Ok(())
}

#[tokio::test]
async fn given_two_instances_when_their_boxes_are_opened_then_the_boxes_differ_and_hold_their_own_files()
-> Res {
    let world = World::new()?;
    let first = world.sandbox.open(instance_id()?, None).await?;
    let second = world.sandbox.open(instance_id()?, None).await?;
    world
        .sandbox
        .write(&first, &path("a.txt")?, b"x".to_vec())
        .await?;

    assert_ne!(first, second);
    assert!(world.sandbox.list(&second, None).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn given_every_operation_when_it_is_called_then_the_record_keeps_the_calls_in_order() -> Res {
    let world = World::new()?;
    let sandbox = world.sandbox.open(instance_id()?, None).await?;
    world
        .sandbox
        .write(&sandbox, &path("a.txt")?, Vec::new())
        .await?;
    world.sandbox.save(&sandbox).await?;
    world.sandbox.release(&sandbox).await?;

    let kinds: Vec<&str> = world
        .record
        .calls()
        .iter()
        .map(|call| match call {
            HarnessCall::SandboxOpen { .. } => "open",
            HarnessCall::SandboxWrite { .. } => "write",
            HarnessCall::SandboxSave { .. } => "save",
            HarnessCall::SandboxRelease { .. } => "release",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["open", "write", "save", "release"]);
    Ok(())
}

#[tokio::test]
async fn given_a_web_access_setting_when_the_box_is_opened_then_the_provider_gets_it_unchanged()
-> Res {
    let world = World::new()?;
    let site = br_agentic_harness_contract::Site::new("example.org")?;
    let setting = WebAccess::Whitelist(vec![site]);

    let sandbox = world.sandbox.open(instance_id()?, Some(&setting)).await?;

    assert_eq!(
        world.sandbox.network(&sandbox),
        Some(Network::Open(setting.clone()))
    );
    let passed = world
        .record
        .calls()
        .into_iter()
        .find_map(|call| match call {
            HarnessCall::SandboxOpen { web, .. } => web,
            _ => None,
        });
    assert_eq!(passed, Some(setting));
    Ok(())
}

#[tokio::test]
async fn given_no_setting_or_a_setting_that_reaches_nothing_when_the_box_is_opened_then_it_has_no_network()
-> Res {
    let world = World::new()?;
    let nothing = WebAccess::None;
    let empty = WebAccess::Whitelist(Vec::new());

    let unset = world.sandbox.open(instance_id()?, None).await?;
    let none = world.sandbox.open(instance_id()?, Some(&nothing)).await?;
    let blank = world.sandbox.open(instance_id()?, Some(&empty)).await?;

    for sandbox in [unset, none, blank] {
        assert_eq!(world.sandbox.network(&sandbox), Some(Network::Closed));
    }
    Ok(())
}
