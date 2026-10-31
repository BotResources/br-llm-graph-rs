use std::time::Duration;

use br_agentic_contract::WorkspaceFile;
use br_agentic_nodes::{
    EmptyList, FallbackList, FileSystem, InvalidLimit, Ladder, ProgrammaticRefusalLimit,
    RoundLimit, SandboxError, SandboxPath, StepLimit, Timeouts,
};

use crate::common::*;

fn file(path: &str) -> WorkspaceFile {
    WorkspaceFile {
        path: path.to_owned(),
        content: path.as_bytes().to_vec(),
    }
}

#[test]
fn given_a_step_limit_of_zero_when_it_is_built_then_it_is_refused_and_one_is_the_least() {
    assert_eq!(StepLimit::new(0), Err(InvalidLimit::Steps));
    assert_eq!(StepLimit::new(1).unwrap().get(), 1);
}

#[test]
fn given_a_round_limit_of_zero_when_it_is_built_then_it_is_refused_and_one_is_the_least() {
    assert_eq!(RoundLimit::new(0), Err(InvalidLimit::Rounds));
    assert_eq!(RoundLimit::new(1).unwrap().get(), 1);
}

#[test]
fn given_a_programmatic_refusal_limit_of_zero_when_it_is_built_then_it_is_refused_and_one_is_the_least()
 {
    assert_eq!(
        ProgrammaticRefusalLimit::new(0),
        Err(InvalidLimit::Refusals)
    );
    assert_eq!(ProgrammaticRefusalLimit::new(1).unwrap().get(), 1);
}

#[test]
fn given_a_zero_timeout_when_the_timeouts_are_built_then_they_are_refused() {
    let second = Duration::from_secs(1);
    assert_eq!(
        Timeouts::new(Duration::ZERO, second),
        Err(InvalidLimit::Timeout)
    );
    assert_eq!(
        Timeouts::new(second, Duration::ZERO),
        Err(InvalidLimit::Timeout)
    );
    let timeouts = Timeouts::new(second, Duration::from_millis(1)).unwrap();
    assert_eq!(timeouts.model_call(), second);
    assert_eq!(timeouts.tool_call(), Duration::from_millis(1));
}

#[test]
fn given_no_client_when_a_fallback_list_or_a_ladder_is_built_then_it_is_refused() {
    assert!(matches!(FallbackList::new(Vec::new()), Err(EmptyList)));
    assert!(matches!(Ladder::new(Vec::new()), Err(EmptyList)));
}

#[test]
fn given_one_client_when_a_fallback_list_is_built_then_it_holds_it() {
    let model = Scripted::new(vec![]);
    let fallback = list(&[&model]);
    assert_eq!(fallback.len(), 1);
    assert!(!fallback.is_empty());
}

#[test]
fn given_a_ladder_of_three_entries_when_a_position_is_past_the_end_then_the_last_entry_serves_it() {
    let model = Scripted::new(vec![]);
    let ladder = ladder(vec![
        list(&[&model]),
        list(&[&model, &model]),
        list(&[&model, &model, &model]),
    ]);

    assert_eq!(ladder.len(), 3);
    assert_eq!(ladder.entry(0).len(), 1);
    assert_eq!(ladder.entry(2).len(), 3);
    assert_eq!(ladder.entry(3).len(), 3);
    assert_eq!(ladder.entry(100).len(), 3);
    assert_eq!(ladder.entry(u32::MAX).len(), 3);
}

#[test]
fn given_a_ladder_of_one_entry_when_any_position_is_asked_then_that_entry_serves_it() {
    let model = Scripted::new(vec![]);
    let ladder = ladder(vec![list(&[&model, &model])]);

    assert_eq!(ladder.entry(0).len(), 2);
    assert_eq!(ladder.entry(7).len(), 2);
}

#[test]
fn given_an_absolute_path_when_a_sandbox_path_is_built_then_it_is_refused() {
    assert!(matches!(
        SandboxPath::new("/etc/passwd"),
        Err(SandboxError::Absolute(_))
    ));
}

#[test]
fn given_a_path_with_a_parent_step_when_a_sandbox_path_is_built_then_it_is_refused() {
    assert!(matches!(
        SandboxPath::new(".."),
        Err(SandboxError::Escapes(_))
    ));
    assert!(matches!(
        SandboxPath::new("a/../b"),
        Err(SandboxError::Escapes(_))
    ));
    assert!(matches!(
        SandboxPath::new("../a"),
        Err(SandboxError::Escapes(_))
    ));
}

#[test]
fn given_an_empty_path_when_a_sandbox_path_is_built_then_it_is_refused() {
    assert!(matches!(SandboxPath::new(""), Err(SandboxError::Empty(_))));
}

#[test]
fn given_a_relative_path_when_a_sandbox_path_is_built_then_it_is_kept() {
    let path = SandboxPath::new("dir/file.txt").unwrap();
    assert_eq!(path.as_str(), "dir/file.txt");
    assert_eq!(path.to_string(), "dir/file.txt");
}

#[test]
fn given_two_files_with_one_path_when_the_file_system_is_built_then_it_is_refused() {
    let built = FileSystem::from_files(vec![file("a.txt"), file("./a.txt")]);
    assert!(matches!(built, Err(SandboxError::Duplicate(_))));
    let same = FileSystem::from_files(vec![file("a.txt"), file("a.txt")]);
    assert!(matches!(same, Err(SandboxError::Duplicate(_))));
}

#[test]
fn given_a_file_with_a_refused_path_when_the_file_system_is_built_then_it_is_refused() {
    assert!(FileSystem::from_files(vec![file("/abs")]).is_err());
    assert!(FileSystem::from_files(vec![file("a/../b")]).is_err());
    assert!(FileSystem::from_files(vec![file("")]).is_err());
}

#[test]
fn given_a_file_system_when_a_path_is_refused_then_no_operation_reaches_the_files() {
    let mut system = FileSystem::from_files(vec![file("a.txt")]).unwrap();

    assert!(system.read("../a.txt").is_err());
    assert!(system.write("/a.txt", Vec::new()).is_err());
    assert!(system.remove("").is_err());
    assert_eq!(system.len(), 1);
}

#[test]
fn given_files_when_the_file_system_is_built_and_given_back_then_the_contents_are_kept() {
    let system = FileSystem::from_files(vec![file("b.txt"), file("dir/a.txt")]).unwrap();

    assert_eq!(system.len(), 2);
    assert_eq!(
        system.read("dir/a.txt").unwrap(),
        Some(b"dir/a.txt".as_slice())
    );
    assert_eq!(system.read("missing").unwrap(), None);
    let mut files = system.into_files();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    assert_eq!(files, vec![file("b.txt"), file("dir/a.txt")]);
}
