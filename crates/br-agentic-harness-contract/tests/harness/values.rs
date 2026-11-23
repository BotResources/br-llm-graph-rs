use br_agentic_harness_contract::{Command, FilePath, InvalidFilePath, SkillName};

#[test]
fn given_a_path_when_it_is_built_then_it_is_relative_normalized_and_stays_inside_the_folder() {
    assert_eq!(
        FilePath::new("a//b/./c.txt").map(|path| path.as_str().to_owned()),
        Ok("a/b/c.txt".to_owned())
    );
    assert!(matches!(
        FilePath::new("/etc/passwd"),
        Err(InvalidFilePath::Absolute(_))
    ));
    assert!(matches!(
        FilePath::new("a/../../b"),
        Err(InvalidFilePath::Escapes(_))
    ));
    assert!(matches!(
        FilePath::new("./"),
        Err(InvalidFilePath::Empty(_))
    ));
    assert!(matches!(
        FilePath::new("a\\b"),
        Err(InvalidFilePath::Character(_))
    ));
    assert!(matches!(
        FilePath::new("a\0b"),
        Err(InvalidFilePath::Character(_))
    ));
}

#[test]
fn given_a_folder_when_paths_are_tested_then_only_paths_inside_it_are_under_it() {
    let folder = FilePath::new("skills/pdf");
    let inside = FilePath::new("skills/pdf/SKILL.md");
    let sibling = FilePath::new("skills/pdf2/SKILL.md");
    assert!(matches!((&folder, &inside), (Ok(folder), Ok(inside)) if inside.is_under(folder)));
    assert!(matches!((&folder, &sibling), (Ok(folder), Ok(sibling)) if !sibling.is_under(folder)));
}

#[test]
fn given_a_blank_text_when_a_skill_name_or_a_command_is_built_then_it_is_refused() {
    assert!(SkillName::new("  ").is_err());
    assert!(SkillName::new("pdf").is_ok());
    assert!(Command::new("\n").is_err());
    assert!(Command::new("ls").is_ok());
}
