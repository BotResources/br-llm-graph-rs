use br_agentic_contract::memory::Call;
use br_agentic_contract::{FileSystemMode, WorkspaceFile};
use br_graph_contract::Event;

use crate::common::*;

fn file(path: &str, content: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: content.as_bytes().to_vec(),
    }
}

fn saves(rig: &Rig) -> Vec<Vec<WorkspaceFile>> {
    rig.record
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::WorkspaceSave { files } => Some(files),
            _ => None,
        })
        .collect()
}

fn sorted(mut files: Vec<WorkspaceFile>) -> Vec<WorkspaceFile> {
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

#[tokio::test(start_paused = true)]
async fn given_the_mode_always_when_the_agent_ends_then_the_loaded_files_are_saved_once_after_the_last_change()
 {
    let files = vec![file("dir/b.txt", "second"), file("a.txt", "first")];
    let rig = Rig::new().with_files(FileSystemMode::Always, files.clone());
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    let update = Driver::result(&node).await;

    assert_eq!(update.text, "done");
    let seen = names(&rig.record);
    assert_eq!(
        seen.iter()
            .filter(|name| **name == "workspace_load")
            .count(),
        1
    );
    assert_eq!(seen.last(), Some(&"workspace_save"));
    let saved = saves(&rig);
    assert_eq!(saved.len(), 1);
    assert_eq!(sorted(saved[0].clone()), sorted(files));
    let load = seen
        .iter()
        .position(|name| *name == "workspace_load")
        .unwrap();
    let first_model_change = seen
        .iter()
        .position(|name| *name == "open_response")
        .unwrap();
    assert!(load < first_model_change);
}

#[tokio::test(start_paused = true)]
async fn given_the_mode_might_and_files_when_the_agent_ends_then_the_files_are_saved_once() {
    let files = vec![file("a.txt", "first")];
    let rig = Rig::new().with_files(FileSystemMode::Might, files.clone());
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let saved = saves(&rig);
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0], files);
}

#[tokio::test(start_paused = true)]
async fn given_the_mode_never_when_the_agent_ends_then_the_workspace_is_neither_loaded_nor_saved() {
    let rig = Rig::new().with_files(FileSystemMode::Never, vec![file("a.txt", "first")]);
    let model = Scripted::new(vec![Reply::text("done")]);
    let node = rig.react(&[&model]);

    Driver::result(&node).await;

    let seen = names(&rig.record);
    assert!(!seen.contains(&"workspace_load"));
    assert!(!seen.contains(&"workspace_save"));
}

async fn refused_load(files: Vec<WorkspaceFile>) {
    let rig = Rig::new().with_files(FileSystemMode::Always, files);
    let model = Scripted::new(vec![Reply::text("never")]);
    let node = rig.react(&[&model]);

    let event = Driver::run(&node).await.1;

    assert!(matches!(event, Event::Failed(_)), "{event:?}");
    assert_eq!(model.calls(), 0);
    assert!(saves(&rig).is_empty());
}

#[tokio::test(start_paused = true)]
async fn given_a_workspace_file_with_an_absolute_path_when_the_instance_starts_then_it_fails_before_any_model_call()
 {
    refused_load(vec![file("/etc/passwd", "x")]).await;
}

#[tokio::test(start_paused = true)]
async fn given_a_workspace_file_that_leaves_the_file_system_when_the_instance_starts_then_it_fails_before_any_model_call()
 {
    refused_load(vec![file("a/../../x", "x")]).await;
}

#[tokio::test(start_paused = true)]
async fn given_two_workspace_files_with_one_path_when_the_instance_starts_then_it_fails_before_any_model_call()
 {
    refused_load(vec![file("a.txt", "one"), file("./a.txt", "two")]).await;
}
