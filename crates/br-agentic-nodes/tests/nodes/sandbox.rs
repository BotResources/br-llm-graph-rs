use br_agentic_contract::WorkspaceFile;
use br_agentic_nodes::{FileSystem, SandboxError, SandboxPath};

#[test]
fn given_a_relative_path_when_it_is_built_then_it_is_normalized() {
    let path = SandboxPath::new("./a//b/./c.txt").unwrap();

    assert_eq!(path.as_str(), "a/b/c.txt");
}

#[test]
fn given_paths_that_leave_the_file_system_when_they_are_built_then_each_is_refused() {
    assert!(matches!(
        SandboxPath::new("../x"),
        Err(SandboxError::Escapes(_))
    ));
    assert!(matches!(
        SandboxPath::new("a/../../x"),
        Err(SandboxError::Escapes(_))
    ));
    assert!(matches!(
        SandboxPath::new("/etc/passwd"),
        Err(SandboxError::Absolute(_))
    ));
    assert!(matches!(SandboxPath::new(""), Err(SandboxError::Empty(_))));
    assert!(matches!(SandboxPath::new("."), Err(SandboxError::Empty(_))));
    assert!(matches!(
        SandboxPath::new("a\\b"),
        Err(SandboxError::Character(_))
    ));
    assert!(matches!(
        SandboxPath::new("a\0b"),
        Err(SandboxError::Character(_))
    ));
}

#[test]
fn given_two_paths_that_name_one_file_when_the_file_system_is_built_then_it_is_refused() {
    let files = vec![
        WorkspaceFile {
            path: "a/b.txt".to_owned(),
            content: Vec::new(),
        },
        WorkspaceFile {
            path: "./a//b.txt".to_owned(),
            content: Vec::new(),
        },
    ];

    assert!(matches!(
        FileSystem::from_files(files),
        Err(SandboxError::Duplicate(_))
    ));
}

#[test]
fn given_a_file_system_when_files_are_written_read_and_removed_then_they_follow() {
    let mut system = FileSystem::from_files(Vec::new()).unwrap();

    system.write("dir/a.txt", b"one".to_vec()).unwrap();
    system.write("./dir/a.txt", b"two".to_vec()).unwrap();

    assert_eq!(system.len(), 1);
    assert_eq!(system.read("dir/a.txt").unwrap(), Some(&b"two"[..]));
    assert!(system.read("../x").is_err());
    assert!(system.remove("dir/a.txt").unwrap());
    assert!(!system.remove("dir/a.txt").unwrap());
    assert!(system.is_empty());
}

#[test]
fn given_a_file_system_when_it_is_turned_into_files_then_the_content_is_kept() {
    let mut system = FileSystem::from_files(Vec::new()).unwrap();
    system.write("a.txt", b"a".to_vec()).unwrap();

    let files = system.into_files();

    assert_eq!(
        files,
        vec![WorkspaceFile {
            path: "a.txt".to_owned(),
            content: b"a".to_vec()
        }]
    );
}
