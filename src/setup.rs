use std::path::{Path, PathBuf};

/// The snippet appended to agent config files.
const SNIPPET: &str = "\n\n## cargo-terse\n\n\
    Use `cargo terse` instead of `cargo` for build, check, test, and clippy commands.\n\
    Examples:\n\
    - `cargo terse check` instead of `cargo check`\n\
    - `cargo terse test` instead of `cargo test`\n\
    - `cargo terse clippy` instead of `cargo clippy`\n\n\
    When a diagnostic needs more context, use `cargo terse detail <ID>` to expand it.\n";

/// Known agent config files and their short names.
const AGENT_FILES: &[(&str, &str)] = &[
    ("claude", "CLAUDE.md"),
    ("agents", "AGENTS.md"),
    ("cursor", ".cursorrules"),
    ("copilot", ".github/copilot-instructions.md"),
];

fn detect_agent_files(dir: &Path) -> Vec<PathBuf> {
    AGENT_FILES
        .iter()
        .map(|(_, file)| dir.join(file))
        .filter(|p| p.exists())
        .collect()
}

fn agent_file_path(dir: &Path, agent: &str) -> Option<PathBuf> {
    AGENT_FILES
        .iter()
        .find(|(name, _)| *name == agent)
        .map(|(_, file)| dir.join(file))
}

fn already_configured(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.contains("cargo terse"))
        .unwrap_or(false)
}

fn append_snippet(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content = std::fs::read_to_string(path).unwrap_or_default();
    content.push_str(SNIPPET);
    std::fs::write(path, content)
}

pub fn run(dir: &Path, global: bool, agent: Option<&str>) -> i32 {
    if global {
        let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            Ok(h) => PathBuf::from(h),
            Err(_) => {
                eprintln!("cargo-terse: cannot determine home directory");
                return 1;
            }
        };
        let path = home.join(".claude").join("CLAUDE.md");
        return write_one(&path);
    }

    if let Some(name) = agent {
        return match agent_file_path(dir, name) {
            Some(path) => write_one(&path),
            None => {
                let known: Vec<&str> = AGENT_FILES.iter().map(|(n, _)| *n).collect();
                eprintln!(
                    "cargo-terse: unknown agent '{}'. known: {}",
                    name,
                    known.join(", ")
                );
                1
            }
        };
    }

    // Auto-detect: write to all existing agent files, or create CLAUDE.md.
    let targets = detect_agent_files(dir);
    if targets.is_empty() {
        return write_one(&dir.join("CLAUDE.md"));
    }

    let mut code = 0;
    for path in &targets {
        let c = write_one(path);
        if c != 0 {
            code = c;
        }
    }
    code
}

fn write_one(path: &Path) -> i32 {
    let display = path.display();
    if already_configured(path) {
        println!("cargo-terse: {} already configured, skipping", display);
        return 0;
    }
    match append_snippet(path) {
        Ok(()) => {
            println!("cargo-terse: appended instructions to {}", display);
            0
        }
        Err(e) => {
            eprintln!("cargo-terse: failed to write {}: {}", display, e);
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_finds_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Claude").unwrap();
        std::fs::write(dir.path().join(".cursorrules"), "rules").unwrap();
        assert_eq!(detect_agent_files(dir.path()).len(), 2);
    }

    #[test]
    fn detect_empty_dir_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect_agent_files(dir.path()).is_empty());
    }

    #[test]
    fn agent_file_path_known() {
        let dir = Path::new("/project");
        assert_eq!(
            agent_file_path(dir, "cursor"),
            Some(PathBuf::from("/project/.cursorrules"))
        );
    }

    #[test]
    fn agent_file_path_unknown() {
        assert!(agent_file_path(Path::new("/project"), "vscode").is_none());
    }

    #[test]
    fn already_configured_detects_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        std::fs::write(&path, "Use `cargo terse` for builds.").unwrap();
        assert!(already_configured(&path));
    }

    #[test]
    fn already_configured_false_for_clean_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        std::fs::write(&path, "# My Project").unwrap();
        assert!(!already_configured(&path));
    }

    #[test]
    fn already_configured_false_for_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!already_configured(&dir.path().join("nope.md")));
    }

    #[test]
    fn append_creates_file_if_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("NEW.md");
        append_snippet(&path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("cargo terse"));
    }

    #[test]
    fn append_preserves_existing_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        std::fs::write(&path, "# Existing").unwrap();
        append_snippet(&path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.starts_with("# Existing"));
        assert!(content.contains("cargo terse"));
    }

    #[test]
    fn append_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".github").join("copilot-instructions.md");
        append_snippet(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn run_auto_detect_creates_claude_md_when_none_exist() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(run(dir.path(), false, None), 0);
        assert!(dir.path().join("CLAUDE.md").exists());
    }

    #[test]
    fn run_auto_detect_writes_to_existing_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".cursorrules"), "").unwrap();
        assert_eq!(run(dir.path(), false, None), 0);
        let content = std::fs::read_to_string(dir.path().join(".cursorrules")).unwrap();
        assert!(content.contains("cargo terse"));
        assert!(!dir.path().join("CLAUDE.md").exists());
    }

    #[test]
    fn run_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), false, None);
        let first = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        run(dir.path(), false, None);
        let second = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn run_agent_flag_creates_specific_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(run(dir.path(), false, Some("copilot")), 0);
        assert!(dir.path().join(".github/copilot-instructions.md").exists());
    }

    #[test]
    fn run_unknown_agent_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(run(dir.path(), false, Some("vscode")), 1);
    }
}
