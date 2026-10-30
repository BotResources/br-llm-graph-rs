use br_agentic_contract::memory::{Call, MemoryWorkspace, Record};
use std::sync::atomic::{AtomicUsize, Ordering};

use br_agentic_contract::{
    FileSystemMode, PortError, PortFuture, WorkspaceFile, WorkspacePort, save_files,
};

use crate::common::*;

fn file(path: &str, content: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: content.as_bytes().to_vec(),
    }
}

fn loads(record: &Record) -> usize {
    record
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::WorkspaceLoad))
        .count()
}

fn saves(record: &Record) -> usize {
    record
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::WorkspaceSave { .. }))
        .count()
}

#[tokio::test]
async fn given_the_mode_always_and_no_file_when_the_node_starts_then_it_has_an_empty_file_system() {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, Vec::new());

    let files = FileSystemMode::Always
        .start(&port, &fields())
        .await
        .unwrap();

    assert_eq!(files, Some(Vec::new()));
    assert_eq!(loads(&record), 1);
}

#[tokio::test]
async fn given_the_mode_always_and_files_when_the_node_starts_then_the_files_are_put_in_the_file_system()
 {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, vec![file("a.txt", "a")]);

    let files = FileSystemMode::Always
        .start(&port, &fields())
        .await
        .unwrap();

    assert_eq!(files, Some(vec![file("a.txt", "a")]));
}

#[tokio::test]
async fn given_the_mode_might_and_no_file_when_the_node_starts_then_it_has_no_file_system() {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, Vec::new());

    let files = FileSystemMode::Might.start(&port, &fields()).await.unwrap();

    assert_eq!(files, None);
    assert_eq!(loads(&record), 1);
}

#[tokio::test]
async fn given_the_mode_might_and_files_when_the_node_starts_then_it_has_a_file_system_with_them() {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, vec![file("a.txt", "a")]);

    let files = FileSystemMode::Might.start(&port, &fields()).await.unwrap();

    assert_eq!(files, Some(vec![file("a.txt", "a")]));
}

#[tokio::test]
async fn given_the_mode_never_when_the_node_starts_then_the_load_is_not_called_and_there_is_no_file_system()
 {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, vec![file("a.txt", "a")]);

    let files = FileSystemMode::Never.start(&port, &fields()).await.unwrap();

    assert_eq!(files, None);
    assert_eq!(loads(&record), 0);
}

#[tokio::test]
async fn given_a_file_system_even_an_empty_one_when_the_node_ends_with_result_then_the_files_are_saved()
 {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, vec![file("old.txt", "old")]);

    save_files(&port, &fields(), Some(Vec::new()))
        .await
        .unwrap();
    assert_eq!(saves(&record), 1);
    assert!(port.files().is_empty());

    save_files(&port, &fields(), Some(vec![file("new.txt", "new")]))
        .await
        .unwrap();
    assert_eq!(port.files(), vec![file("new.txt", "new")]);
}

#[tokio::test]
async fn given_no_file_system_when_the_node_ends_with_result_then_nothing_is_saved() {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, Vec::new());

    save_files(&port, &fields(), None).await.unwrap();

    assert_eq!(saves(&record), 0);
}

#[tokio::test]
async fn given_the_workspace_that_has_saved_files_when_a_later_node_loads_then_it_gets_the_saved_content()
 {
    let record = Record::new();
    let port = MemoryWorkspace::new(&record, Vec::new());
    save_files(&port, &fields(), Some(vec![file("k", "kept")]))
        .await
        .unwrap();

    let loaded = WorkspacePort::<Doc>::load(&port, &fields()).await.unwrap();

    assert_eq!(loaded, vec![file("k", "kept")]);
}

struct Broken {
    calls: AtomicUsize,
}

impl WorkspacePort<Doc> for Broken {
    fn load<'a>(
        &'a self,
        _snapshot: &'a Doc,
    ) -> PortFuture<'a, Result<Vec<WorkspaceFile>, PortError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(PortError::fault("workspace unreachable")) })
    }

    fn save<'a>(
        &'a self,
        _snapshot: &'a Doc,
        _files: Vec<WorkspaceFile>,
    ) -> PortFuture<'a, Result<(), PortError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(PortError::fault("workspace unreachable")) })
    }
}

#[tokio::test]
async fn given_a_workspace_port_that_fails_when_the_node_starts_then_the_failure_is_a_fault_and_the_load_is_called_once()
 {
    let port = Broken {
        calls: AtomicUsize::new(0),
    };

    let started = FileSystemMode::Always.start(&port, &fields()).await;

    assert!(matches!(started, Err(PortError::Fault(_))));
    assert_eq!(port.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn given_a_workspace_port_that_fails_when_the_node_saves_then_the_failure_is_a_fault_and_the_save_is_called_once()
 {
    let port = Broken {
        calls: AtomicUsize::new(0),
    };

    let saved = save_files(&port, &fields(), Some(Vec::new())).await;

    assert!(matches!(saved, Err(PortError::Fault(_))));
    assert_eq!(port.calls.load(Ordering::SeqCst), 1);
}
