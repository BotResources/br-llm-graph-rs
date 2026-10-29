use br_agentic_contract::memory::MemoryWorkspace;
use br_agentic_contract::{FileSystemMode, WorkspaceFile, save_files};

use crate::common::Doc;

fn file(path: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: path.as_bytes().to_vec(),
    }
}

#[tokio::test]
async fn given_the_mode_always_when_the_load_gives_no_files_then_the_node_still_has_a_file_system_that_starts_empty()
 {
    let port = MemoryWorkspace::new(Vec::new());

    let started = FileSystemMode::Always
        .start::<Doc>(&port, &Doc { n: 0 })
        .await
        .unwrap();

    assert_eq!(started, Some(Vec::new()));
}

#[tokio::test]
async fn given_the_mode_might_when_the_load_gives_files_then_the_node_has_a_file_system_with_them()
{
    let port = MemoryWorkspace::new(vec![file("a.txt")]);

    let started = FileSystemMode::Might
        .start::<Doc>(&port, &Doc { n: 0 })
        .await
        .unwrap();

    assert_eq!(started, Some(vec![file("a.txt")]));
}

#[tokio::test]
async fn given_the_mode_might_when_the_load_gives_no_files_then_the_node_has_no_file_system() {
    let port = MemoryWorkspace::new(Vec::new());

    let started = FileSystemMode::Might
        .start::<Doc>(&port, &Doc { n: 0 })
        .await
        .unwrap();

    assert_eq!(started, None);
}

#[tokio::test]
async fn given_the_mode_never_when_the_instance_starts_then_the_load_is_not_asked_and_there_is_no_file_system()
 {
    let port = MemoryWorkspace::new(vec![file("a.txt")]);

    let started = FileSystemMode::Never
        .start::<Doc>(&port, &Doc { n: 0 })
        .await
        .unwrap();

    assert_eq!(started, None);
    assert_eq!(port.loads(), 0);
}

#[tokio::test]
async fn given_a_file_system_when_the_instance_ends_with_a_result_then_the_files_are_saved() {
    let port = MemoryWorkspace::new(Vec::new());

    save_files::<Doc>(&port, &Doc { n: 0 }, Some(vec![file("out.txt")]))
        .await
        .unwrap();

    assert_eq!(port.saves(), 1);
    assert_eq!(port.files(), vec![file("out.txt")]);
}

#[tokio::test]
async fn given_no_file_system_when_the_instance_ends_with_a_result_then_nothing_is_saved() {
    let port = MemoryWorkspace::new(Vec::new());

    save_files::<Doc>(&port, &Doc { n: 0 }, None).await.unwrap();

    assert_eq!(port.saves(), 0);
}
