use br_agentic_contract::{FileSystemMode, WorkspaceFile};
use br_llm_messages::ClosureReason;

use crate::common::*;
use crate::kit::*;

fn file(path: &str, content: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: content.as_bytes().to_vec(),
    }
}

fn interrupted(reason: &Option<ClosureReason>) -> bool {
    matches!(reason, Some(ClosureReason::Interrupted { .. }))
}

#[tokio::test(start_paused = true)]
async fn given_no_file_system_when_the_mode_is_never_then_the_workspace_is_neither_loaded_nor_saved()
 {
    let rig = Rig::new();
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(count(&rig.record, "workspace_load"), 0);
    assert_eq!(count(&rig.record, "workspace_save"), 0);
}

#[tokio::test(start_paused = true)]
async fn given_the_mode_always_when_the_instance_runs_then_the_files_are_loaded_at_start_and_saved_at_the_end()
 {
    let files = vec![file("notes.txt", "hello")];
    let rig = Rig::new().with_files(FileSystemMode::Always, files.clone());
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(count(&rig.record, "workspace_load"), 1);
    assert!(position(&rig.record, "workspace_load") < position(&rig.record, "open_response"));
    assert_eq!(saves(&rig.record), vec![files]);
}

#[tokio::test(start_paused = true)]
async fn given_the_mode_always_when_the_workspace_holds_no_file_then_a_file_system_exists_and_is_saved()
 {
    let rig = Rig::new().with_files(FileSystemMode::Always, Vec::new());
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(count(&rig.record, "workspace_load"), 1);
    assert_eq!(saves(&rig.record), vec![Vec::<WorkspaceFile>::new()]);
}

#[tokio::test(start_paused = true)]
async fn given_the_mode_might_when_the_workspace_holds_files_then_the_file_system_exists_and_is_saved()
 {
    let files = vec![file("notes.txt", "hello")];
    let rig = Rig::new().with_files(FileSystemMode::Might, files.clone());
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(count(&rig.record, "workspace_load"), 1);
    assert_eq!(saves(&rig.record), vec![files]);
}

#[tokio::test(start_paused = true)]
async fn given_the_mode_might_when_the_workspace_holds_no_file_then_the_node_has_no_file_system_and_saves_nothing()
 {
    let rig = Rig::new().with_files(FileSystemMode::Might, Vec::new());
    let model = Model::new(vec![Act::text("done")]);
    let node = node(&rig, plains(&[&model]));

    Driver::result(&node).await;

    assert_eq!(count(&rig.record, "workspace_load"), 1);
    assert_eq!(count(&rig.record, "workspace_save"), 0);
}
