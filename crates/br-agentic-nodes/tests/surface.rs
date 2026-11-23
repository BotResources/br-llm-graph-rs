use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

fn sources_in(dir: &Path, found: &mut Vec<(PathBuf, String)>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            sources_in(&path, found)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = fs::read_to_string(&path)?;
            found.push((path, text));
        }
    }
    Ok(())
}

fn sources() -> Result<Vec<(PathBuf, String)>, Box<dyn Error>> {
    let mut found = Vec::new();
    sources_in(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    )?;
    Ok(found)
}

#[test]
fn given_the_library_when_its_sources_are_read_then_it_defines_no_port()
-> Result<(), Box<dyn Error>> {
    for (path, text) in sources()? {
        for line in text.lines().map(str::trim_start) {
            let declaration = line.strip_prefix("pub ").unwrap_or(line);
            let declaration = declaration.strip_prefix("(crate) ").unwrap_or(declaration);
            assert!(
                !declaration.starts_with("trait ") && !line.contains("#[async_trait"),
                "{line} in {}",
                path.display()
            );
        }
        for name in [
            "PromptPort",
            "OutputSchemaPort",
            "AuthorPort",
            "ReactOutcomePort",
            "GeneratorCriticOutcomePort",
            "CriticRequestPort",
            "ProgrammaticCriticPort",
            "ProgrammaticVerdict",
            "ReactOutcome",
            "GeneratorCriticOutcome",
            "Judgement",
            "CriticRequest",
        ] {
            for keyword in ["struct", "enum", "type", "trait"] {
                let definition = format!("{keyword} {name}");
                let defined = text.match_indices(&definition).any(|(at, _)| {
                    !text[at + definition.len()..]
                        .starts_with(|next: char| next.is_alphanumeric() || next == '_')
                });
                assert!(!defined, "{definition} in {}", path.display());
            }
        }
        assert!(!text.contains("fn structured_reply"), "{}", path.display());
    }
    Ok(())
}

#[test]
fn given_the_library_when_its_sources_are_read_then_no_type_of_the_file_system_the_sandbox_or_the_mail_tools_exists()
-> Result<(), Box<dyn Error>> {
    for (path, text) in sources()? {
        for name in [
            "FileSystem",
            "SandboxPath",
            "SandboxError",
            "SEND_MAIL",
            "GET_MAIL_FILES",
            "WorkspacePort",
        ] {
            assert!(!text.contains(name), "{name} in {}", path.display());
        }
    }
    Ok(())
}
