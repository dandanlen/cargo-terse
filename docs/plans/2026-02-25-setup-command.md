# Setup Command Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `cargo terse setup` to auto-configure AI agent instruction files with cargo-terse usage instructions.

**Architecture:** New `Command::Setup` variant in cli.rs, new `setup.rs` module containing snippet, agent file detection, idempotency, and append logic. Pure filesystem operations, no external dependencies.

**Tech Stack:** Rust stdlib only (std::fs, std::path, std::env)

---

### Task 1: Add Setup variant to CLI parser

**Files:**
- Modify: `src/cli.rs:18-31` (Command enum)
- Modify: `src/cli.rs:63-91` (parse_args Value branch)
- Test: `src/cli.rs` (tests module)

**Step 1: Write the failing tests**

Add to the `tests` module in `src/cli.rs`:

```rust
// 12. setup subcommand
#[test]
fn setup_subcommand() {
    let cmd = parse_args(args(&["cargo-terse", "terse", "setup"])).unwrap();
    match cmd {
        Command::Setup { global, agent } => {
            assert!(!global);
            assert!(agent.is_none());
        }
        _ => panic!("expected Setup command"),
    }
}

// 13. setup --global
#[test]
fn setup_global() {
    let cmd = parse_args(args(&["cargo-terse", "terse", "setup", "--global"])).unwrap();
    match cmd {
        Command::Setup { global, agent } => {
            assert!(global);
            assert!(agent.is_none());
        }
        _ => panic!("expected Setup command"),
    }
}

// 14. setup --agent cursor
#[test]
fn setup_agent_flag() {
    let cmd = parse_args(args(&["cargo-terse", "terse", "setup", "--agent", "cursor"])).unwrap();
    match cmd {
        Command::Setup { global, agent } => {
            assert!(!global);
            assert_eq!(agent.as_deref(), Some("cursor"));
        }
        _ => panic!("expected Setup command"),
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo terse test -- cli::tests`
Expected: FAIL — `Command::Setup` doesn't exist yet.

**Step 3: Add Command::Setup variant and parsing**

In `src/cli.rs`, add to the `Command` enum (after `Help`):

```rust
Setup {
    global: bool,
    agent: Option<String>,
},
```

In `parse_args`, add a new branch in the `if cargo_cmd.is_none()` block, after the `detail` branch and before the cargo subcommand fallthrough:

```rust
if s == "setup" {
    let mut global = false;
    let mut agent: Option<String> = None;
    while let Some(a) = parser.next()? {
        match a {
            lexopt::Arg::Long("global") => global = true,
            lexopt::Arg::Long("agent") => {
                agent = Some(
                    parser.value()?.to_str().unwrap_or("").to_string(),
                );
            }
            other => return Err(other.unexpected()),
        }
    }
    return Ok(Command::Setup { global, agent });
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo terse test -- cli::tests`
Expected: All 14 tests PASS.

**Step 5: Commit**

```bash
git add src/cli.rs
git commit -m "feat(setup): add Setup command variant and CLI parsing"
```

---

### Task 2: Create setup.rs module with snippet and agent config detection

**Files:**
- Create: `src/setup.rs`
- Modify: `src/main.rs:1-7` (add `mod setup;`)

**Step 1: Write the failing tests**

Create `src/setup.rs` with tests first:

```rust
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

/// Detect which agent config files exist in `dir`.
fn detect_agent_files(dir: &Path) -> Vec<PathBuf> {
    AGENT_FILES
        .iter()
        .map(|(_, file)| dir.join(file))
        .filter(|p| p.exists())
        .collect()
}

/// Resolve agent name to its file path relative to `dir`.
fn agent_file_path(dir: &Path, agent: &str) -> Option<PathBuf> {
    AGENT_FILES
        .iter()
        .find(|(name, _)| *name == agent)
        .map(|(_, file)| dir.join(file))
}

/// Returns true if the file already contains cargo-terse instructions.
fn already_configured(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.contains("cargo terse"))
        .unwrap_or(false)
}

/// Append the snippet to the file, creating parent dirs if needed.
fn append_snippet(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut content = std::fs::read_to_string(path).unwrap_or_default();
    content.push_str(SNIPPET);
    std::fs::write(path, content)
}

/// Run setup for the given directory. Returns exit code.
pub fn run(dir: &Path, global: bool, agent: Option<&str>) -> i32 {
    if global {
        let home = match std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
        {
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

        let found = detect_agent_files(dir.path());
        assert_eq!(found.len(), 2);
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
        let code = run(dir.path(), false, None);
        assert_eq!(code, 0);
        assert!(dir.path().join("CLAUDE.md").exists());
    }

    #[test]
    fn run_auto_detect_writes_to_existing_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".cursorrules"), "").unwrap();
        let code = run(dir.path(), false, None);
        assert_eq!(code, 0);
        let content = std::fs::read_to_string(dir.path().join(".cursorrules")).unwrap();
        assert!(content.contains("cargo terse"));
        // Should NOT create CLAUDE.md since .cursorrules existed
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
        let code = run(dir.path(), false, Some("copilot"));
        assert_eq!(code, 0);
        assert!(dir
            .path()
            .join(".github/copilot-instructions.md")
            .exists());
    }

    #[test]
    fn run_unknown_agent_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let code = run(dir.path(), false, Some("vscode"));
        assert_eq!(code, 1);
    }
}
```

**Step 2: Register the module**

In `src/main.rs`, add `mod setup;` after the existing module declarations (line 7).

**Step 3: Run tests to verify they pass**

Run: `cargo terse test -- setup::tests`
Expected: All 14 setup tests PASS.

**Step 4: Commit**

```bash
git add src/setup.rs src/main.rs
git commit -m "feat(setup): add setup module with snippet, detection, and idempotency"
```

---

### Task 3: Wire Setup command into main.rs and update help

**Files:**
- Modify: `src/main.rs:9-31` (help text)
- Modify: `src/main.rs:46-75` (command dispatch)

**Step 1: Add setup to help text**

In `print_help()`, add `setup` to the COMMANDS section after `detail`:

```
    setup       Configure AI agent instruction files
```

**Step 2: Add dispatch for Setup command**

In the `match cmd` block in `main()`, after the `Help` arm:

```rust
cli::Command::Setup { global, agent } => {
    let dir = std::env::current_dir().unwrap_or_default();
    std::process::exit(setup::run(
        &dir,
        global,
        agent.as_deref(),
    ));
}
```

**Step 3: Run full test suite**

Run: `cargo terse test`
Expected: All tests PASS (47 existing + 3 cli + 14 setup = 64).

**Step 4: Run clippy and fmt**

Run: `cargo terse clippy && cargo terse fmt`
Expected: Clean.

**Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(setup): wire setup command into main and help text"
```

---

### Task 4: Update README

**Files:**
- Modify: `README.md`

**Step 1: Add setup to Usage section**

After the existing usage examples (line 34), add:

```markdown

### Setup

```bash
cargo terse setup              # auto-detect and configure agent instruction files
cargo terse setup --global     # configure ~/.claude/CLAUDE.md
cargo terse setup --agent cursor  # configure .cursorrules specifically
```

**Step 2: Commit**

```bash
git add README.md
git commit -m "docs: add setup command to README"
```
