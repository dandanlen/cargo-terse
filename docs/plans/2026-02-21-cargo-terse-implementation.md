# cargo-terse Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a cargo plugin that wraps cargo build/check/test/clippy/fmt with concise, AI-optimized output in plain text, JSON, or TOON format.

**Architecture:** A binary `cargo-terse` that spawns cargo with `--message-format=json`, parses the JSON stream, and re-renders diagnostics in a condensed format. Test results are parsed from stderr text. A cache file enables drill-down into full diagnostics by ID.

**Tech Stack:** Rust, lexopt (CLI), serde + serde_json (JSON parsing), toon-format (TOON output)

**Design doc:** `docs/plans/2026-02-21-cargo-terse-design.md`

---

### Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

**Step 1: Initialize the cargo project**

Run: `cargo init --name cargo-terse /Users/dan/Work/projects/cargo-terse`
Expected: Creates Cargo.toml and src/main.rs (may warn about existing files, that's fine)

**Step 2: Set up Cargo.toml dependencies**

`Cargo.toml` should contain:
```toml
[package]
name = "cargo-terse"
version = "0.1.0"
edition = "2021"
description = "Concise cargo output for AI-assisted workflows"
license = "MIT"

[dependencies]
lexopt = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toon-format = "0.2"
```

**Step 3: Minimal main.rs that prints version**

```rust
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--version") {
        println!("cargo-terse {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    eprintln!("cargo-terse: not yet implemented");
    std::process::exit(1);
}
```

**Step 4: Verify it compiles and runs**

Run: `cargo build`
Expected: Compiles with no errors

Run: `cargo run -- --version`
Expected: `cargo-terse 0.1.0`

**Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock src/main.rs
git commit -m "feat: project scaffold with dependencies"
```

---

### Task 2: CLI Argument Parsing

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

**Step 1: Write tests for CLI parsing**

Add to `src/cli.rs`:
```rust
use std::ffi::OsString;

#[derive(Debug, PartialEq)]
pub enum OutputFormat {
    Plain,
    Json,
    Toon,
}

#[derive(Debug, PartialEq)]
pub enum Verbosity {
    Terse,
    Verbose,
    VeryVerbose,
}

#[derive(Debug)]
pub enum Command {
    Run {
        cargo_cmd: String,
        format: OutputFormat,
        verbosity: Verbosity,
        no_cache: bool,
        cargo_args: Vec<OsString>,
    },
    Detail {
        id: String,
        format: OutputFormat,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, lexopt::Error> {
        let strs: Vec<OsString> = args.iter().map(OsString::from).collect();
        parse_args(strs)
    }

    #[test]
    fn bare_cargo_terse_defaults_to_check() {
        let cmd = parse(&["cargo-terse", "terse"]).unwrap();
        match cmd {
            Command::Run { cargo_cmd, format, verbosity, no_cache, cargo_args } => {
                assert_eq!(cargo_cmd, "check");
                assert_eq!(format, OutputFormat::Plain);
                assert_eq!(verbosity, Verbosity::Terse);
                assert!(!no_cache);
                assert!(cargo_args.is_empty());
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parse_clippy_with_flags() {
        let cmd = parse(&["cargo-terse", "terse", "--format", "json", "-v", "clippy", "--", "-W", "clippy::all"]).unwrap();
        match cmd {
            Command::Run { cargo_cmd, format, verbosity, cargo_args, .. } => {
                assert_eq!(cargo_cmd, "clippy");
                assert_eq!(format, OutputFormat::Json);
                assert_eq!(verbosity, Verbosity::Verbose);
                assert!(cargo_args.iter().any(|a| a == "-W"));
            }
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parse_detail_command() {
        let cmd = parse(&["cargo-terse", "terse", "detail", "W3"]).unwrap();
        match cmd {
            Command::Detail { id, format } => {
                assert_eq!(id, "W3");
                assert_eq!(format, OutputFormat::Plain);
            }
            _ => panic!("expected Detail"),
        }
    }

    #[test]
    fn parse_vv_verbosity() {
        let cmd = parse(&["cargo-terse", "terse", "-vv", "test"]).unwrap();
        match cmd {
            Command::Run { verbosity, .. } => assert_eq!(verbosity, Verbosity::VeryVerbose),
            _ => panic!("expected Run"),
        }
    }

    #[test]
    fn parse_no_cache() {
        let cmd = parse(&["cargo-terse", "terse", "--no-cache", "build"]).unwrap();
        match cmd {
            Command::Run { no_cache, .. } => assert!(no_cache),
            _ => panic!("expected Run"),
        }
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL — `parse_args` function doesn't exist yet

**Step 3: Implement parse_args**

Add to `src/cli.rs` above the tests:
```rust
pub fn parse_args(args: Vec<OsString>) -> Result<Command, lexopt::Error> {
    use lexopt::prelude::*;

    let mut parser = lexopt::Parser::from_args(args);
    let mut format = OutputFormat::Plain;
    let mut verbosity = Verbosity::Terse;
    let mut no_cache = false;
    let mut subcommand: Option<String> = None;
    let mut cargo_args: Vec<OsString> = Vec::new();

    // Skip "terse" if present (cargo passes the subcommand name as first arg)
    // We need to consume it but not treat it as the cargo command

    let mut seen_terse = false;

    while let Some(arg) = parser.next()? {
        match arg {
            Short('v') => {
                verbosity = match verbosity {
                    Verbosity::Terse => Verbosity::Verbose,
                    _ => Verbosity::VeryVerbose,
                };
            }
            Long("vv") => {
                verbosity = Verbosity::VeryVerbose;
            }
            Long("format") => {
                let val: String = parser.value()?.string()?;
                format = match val.as_str() {
                    "plain" => OutputFormat::Plain,
                    "json" => OutputFormat::Json,
                    "toon" => OutputFormat::Toon,
                    other => return Err(lexopt::Error::UnexpectedValue {
                        option: "format".to_string(),
                        value: other.into(),
                    }),
                };
            }
            Long("no-cache") => {
                no_cache = true;
            }
            Value(val) => {
                let s = val.string()?;
                if !seen_terse && s == "terse" {
                    seen_terse = true;
                    continue;
                }
                if subcommand.is_none() {
                    if s == "detail" {
                        // detail command: next value is the ID
                        let id_arg = parser.next().ok_or_else(|| lexopt::Error::MissingValue {
                            option: Some("detail".to_string()),
                        })??;
                        let id = match id_arg {
                            Value(v) => v.string()?,
                            _ => return Err(lexopt::Error::MissingValue {
                                option: Some("detail".to_string()),
                            }),
                        };
                        return Ok(Command::Detail { id, format });
                    }
                    subcommand = Some(s);
                } else {
                    cargo_args.push(s.into());
                }
            }
            _ => {
                // Pass through unknown flags to cargo
                cargo_args.push(arg.unexpected().to_string().into());
            }
        }
    }

    Ok(Command::Run {
        cargo_cmd: subcommand.unwrap_or_else(|| "check".to_string()),
        format,
        verbosity,
        no_cache,
        cargo_args,
    })
}
```

Note: The `_` arm for unknown args needs refinement — lexopt doesn't make it trivial to reconstruct the original flag. The approach above is a starting point; we may need to use `parser.raw_args()` or collect remaining args after `--`. Refine during implementation to ensure passthrough works correctly with real cargo flags like `--release`, `--workspace`, etc.

**Step 4: Wire up main.rs**

```rust
mod cli;

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    match cli::parse_args(args) {
        Ok(cmd) => {
            eprintln!("Parsed: {:?}", cmd);
            eprintln!("cargo-terse: not yet implemented");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("cargo-terse: {e}");
            std::process::exit(2);
        }
    }
}
```

**Step 5: Run tests to verify they pass**

Run: `cargo test`
Expected: All 5 CLI tests pass

**Step 6: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: CLI argument parsing with lexopt"
```

---

### Task 3: Data Model

**Files:**
- Create: `src/diagnostic.rs`

**Step 1: Define the core types**

These types bridge cargo's JSON output and our formatters:

```rust
use serde::{Deserialize, Serialize};

/// A single diagnostic (warning or error) extracted from cargo JSON output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub id: String,
    pub level: DiagnosticLevel,
    pub code: Option<String>,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<usize>,
    pub col: Option<usize>,
    /// Primary source text span (for -v output)
    pub span_text: Option<String>,
    pub span_label: Option<String>,
    /// Full rendered output from rustc (for -vv output and detail command)
    pub rendered: Option<String>,
    /// Full original JSON from cargo (for cache)
    pub raw_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
}

/// A test result extracted from libtest stderr output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub id: String,
    pub name: String,
    pub passed: bool,
    /// Failure message (only for failed tests, -v)
    pub failure_message: Option<String>,
    /// Source location of assertion (if extractable)
    pub file: Option<String>,
    pub line: Option<usize>,
}

/// Summary of a cargo-terse run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    pub command: String,
    pub success: bool,
    pub errors: usize,
    pub warnings: usize,
    pub tests_passed: usize,
    pub tests_failed: usize,
    pub tests_ignored: usize,
    pub elapsed_secs: f64,
}

/// Everything from a single run — used for cache and formatting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub diagnostics: Vec<Diagnostic>,
    pub test_results: Vec<TestResult>,
    pub summary: RunSummary,
}
```

**Step 2: Verify it compiles**

Add `mod diagnostic;` to `main.rs`.

Run: `cargo build`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/diagnostic.rs src/main.rs
git commit -m "feat: core data model types"
```

---

### Task 4: Diagnostic Parser (Cargo JSON)

**Files:**
- Create: `src/parser.rs`
- Create: `tests/fixtures/` (test fixture JSON files)

**Step 1: Create test fixtures**

Create `tests/fixtures/clippy_output.json` — a realistic multi-line JSON output from `cargo clippy --message-format=json`. Each line is a JSON object. Include:
- 1 `compiler-artifact` line (should be ignored)
- 2 `compiler-message` lines: one warning, one error
- 1 `build-finished` line

Example (each line is a complete JSON object):
```json
{"reason":"compiler-artifact","package_id":"file:///demo#0.1.0","manifest_path":"/demo/Cargo.toml","target":{"kind":["lib"],"name":"demo","src_path":"/demo/src/lib.rs"},"profile":{"opt_level":"0","debuginfo":2},"features":[],"filenames":["/demo/target/debug/libdemo.rlib"],"executable":null,"fresh":false}
{"reason":"compiler-message","package_id":"file:///demo#0.1.0","manifest_path":"/demo/Cargo.toml","target":{"kind":["lib"],"name":"demo","src_path":"/demo/src/lib.rs"},"message":{"message":"unnecessary `return` statement","code":{"code":"clippy::needless_return","explanation":null},"level":"warning","spans":[{"file_name":"src/lib.rs","byte_start":100,"byte_end":118,"line_start":42,"line_end":42,"column_start":5,"column_end":23,"is_primary":true,"text":[{"text":"    return Ok(value);","highlight_start":5,"highlight_end":23}],"label":"help: remove `return`: `Ok(value)`","suggested_replacement":"Ok(value)","suggestion_applicability":"MachineApplicable","expansion":null}],"children":[],"rendered":"warning: unnecessary `return` statement\n  --> src/lib.rs:42:5\n   |\n42 |     return Ok(value);\n   |     ^^^^^^^^^^^^^^^^^ help: remove `return`: `Ok(value)`\n   |\n   = note: `#[warn(clippy::needless_return)]` on by default\n"}}
{"reason":"compiler-message","package_id":"file:///demo#0.1.0","manifest_path":"/demo/Cargo.toml","target":{"kind":["lib"],"name":"demo","src_path":"/demo/src/lib.rs"},"message":{"message":"expected `u32`, found `&str`","code":{"code":"E0308","explanation":null},"level":"error","spans":[{"file_name":"src/handler.rs","byte_start":200,"byte_end":210,"line_start":93,"line_end":93,"column_start":12,"column_end":22,"is_primary":true,"text":[{"text":"    let x: u32 = \"hello\";","highlight_start":12,"highlight_end":22}],"label":"expected `u32`, found `&str`","suggested_replacement":null,"suggestion_applicability":null,"expansion":null}],"children":[],"rendered":"error[E0308]: expected `u32`, found `&str`\n  --> src/handler.rs:93:12\n   |\n93 |     let x: u32 = \"hello\";\n   |            ---   ^^^^^^^ expected `u32`, found `&str`\n   |\n"}}
{"reason":"build-finished","success":false}
```

**Step 2: Write tests for the parser**

```rust
use crate::diagnostic::{Diagnostic, DiagnosticLevel};

/// Parse a single JSON line from cargo's --message-format=json output.
/// Returns Some(Diagnostic) for compiler-message lines, None for others.
pub fn parse_cargo_json_line(line: &str, next_warning_id: &mut usize, next_error_id: &mut usize) -> Option<Diagnostic> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_compiler_artifact() {
        let line = r#"{"reason":"compiler-artifact","package_id":"demo","manifest_path":"x","target":{"kind":["lib"],"name":"demo","src_path":"x"},"profile":{"opt_level":"0","debuginfo":2},"features":[],"filenames":[],"executable":null,"fresh":false}"#;
        let mut w = 1;
        let mut e = 1;
        assert!(parse_cargo_json_line(line, &mut w, &mut e).is_none());
    }

    #[test]
    fn parses_warning() {
        let line = std::fs::read_to_string("tests/fixtures/clippy_output.json")
            .unwrap()
            .lines()
            .nth(1)
            .unwrap()
            .to_string();
        let mut w = 1;
        let mut e = 1;
        let diag = parse_cargo_json_line(&line, &mut w, &mut e).unwrap();
        assert_eq!(diag.id, "W1");
        assert_eq!(diag.level, DiagnosticLevel::Warning);
        assert_eq!(diag.code.as_deref(), Some("clippy::needless_return"));
        assert_eq!(diag.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(diag.line, Some(42));
        assert_eq!(diag.col, Some(5));
        assert_eq!(diag.message, "unnecessary `return` statement");
        assert!(diag.rendered.is_some());
    }

    #[test]
    fn parses_error() {
        let line = std::fs::read_to_string("tests/fixtures/clippy_output.json")
            .unwrap()
            .lines()
            .nth(2)
            .unwrap()
            .to_string();
        let mut w = 1;
        let mut e = 1;
        let diag = parse_cargo_json_line(&line, &mut w, &mut e).unwrap();
        assert_eq!(diag.id, "E1");
        assert_eq!(diag.level, DiagnosticLevel::Error);
        assert_eq!(diag.code.as_deref(), Some("E0308"));
        assert_eq!(diag.file.as_deref(), Some("src/handler.rs"));
        assert_eq!(diag.line, Some(93));
    }

    #[test]
    fn increments_ids() {
        let lines: Vec<String> = std::fs::read_to_string("tests/fixtures/clippy_output.json")
            .unwrap()
            .lines()
            .map(String::from)
            .collect();
        let mut w = 1;
        let mut e = 1;
        // First warning
        let d1 = parse_cargo_json_line(&lines[1], &mut w, &mut e).unwrap();
        assert_eq!(d1.id, "W1");
        assert_eq!(w, 2);
        // Error
        let d2 = parse_cargo_json_line(&lines[2], &mut w, &mut e).unwrap();
        assert_eq!(d2.id, "E1");
        assert_eq!(e, 2);
    }
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL — `parse_cargo_json_line` returns `todo!()`

**Step 4: Implement the parser**

Implement `parse_cargo_json_line` in `src/parser.rs`. Use `serde_json::Value` to parse the line, check `reason == "compiler-message"`, then extract fields from the `message` object. Map the primary span to get file/line/col and span text.

The implementation should:
- Return `None` for non-`compiler-message` reasons
- Return `None` for lines that don't parse as JSON (build scripts can print arbitrary text)
- Extract the primary span (where `is_primary == true`) for file/line/col
- Extract `span_text` from the first text entry's `text` field
- Extract `span_label` from the primary span's `label` field
- Store `rendered` for -vv and cache
- Assign `W{n}` or `E{n}` IDs based on level, incrementing the counters
- Skip diagnostics with level "note" or "help" (they're children of other diagnostics)

**Step 5: Run tests to verify they pass**

Run: `cargo test`
Expected: All parser tests pass

**Step 6: Commit**

```bash
git add src/parser.rs tests/fixtures/
git commit -m "feat: cargo JSON diagnostic parser"
```

---

### Task 5: Test Result Parser (stderr)

**Files:**
- Modify: `src/parser.rs`
- Create: `tests/fixtures/test_stderr_pass.txt`
- Create: `tests/fixtures/test_stderr_fail.txt`

**Step 1: Create test fixtures**

`tests/fixtures/test_stderr_pass.txt`:
```

running 3 tests
test tests::it_works ... ok
test tests::another ... ok
test tests::third ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

```

`tests/fixtures/test_stderr_fail.txt`:
```

running 4 tests
test tests::it_works ... ok
test tests::parse_config_missing_field ... FAILED
test tests::handler_timeout ... FAILED
test tests::another ... ok

failures:

---- tests::parse_config_missing_field stdout ----
thread 'tests::parse_config_missing_field' panicked at src/config.rs:156:5:
assertion `left == right` failed
  left: None
 right: Some("default")

---- tests::handler_timeout stdout ----
thread 'tests::handler_timeout' panicked at src/handler.rs:203:5:
assertion failed: elapsed < Duration::from_secs(5)

failures:
    tests::parse_config_missing_field
    tests::handler_timeout

test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.23s

```

**Step 2: Write tests**

```rust
#[test]
fn parse_passing_test_output() {
    let stderr = std::fs::read_to_string("tests/fixtures/test_stderr_pass.txt").unwrap();
    let (results, summary) = parse_test_stderr(&stderr);
    assert!(results.is_empty()); // No failures to report
    assert_eq!(summary.tests_passed, 3);
    assert_eq!(summary.tests_failed, 0);
}

#[test]
fn parse_failing_test_output() {
    let stderr = std::fs::read_to_string("tests/fixtures/test_stderr_fail.txt").unwrap();
    let (results, summary) = parse_test_stderr(&stderr);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, "F1");
    assert_eq!(results[0].name, "tests::parse_config_missing_field");
    assert!(!results[0].passed);
    assert_eq!(results[0].file.as_deref(), Some("src/config.rs"));
    assert_eq!(results[0].line, Some(156));
    assert!(results[0].failure_message.as_ref().unwrap().contains("left == right"));
    assert_eq!(results[1].id, "F2");
    assert_eq!(summary.tests_passed, 2);
    assert_eq!(summary.tests_failed, 2);
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL — `parse_test_stderr` doesn't exist

**Step 4: Implement `parse_test_stderr`**

```rust
use crate::diagnostic::TestResult;

/// Parse libtest's stderr output into TestResults (failures only) and counts.
/// Returns (failed_tests, partial_summary) where partial_summary has test counts filled in.
pub fn parse_test_stderr(stderr: &str) -> (Vec<TestResult>, TestSummary) {
    // ...
}

pub struct TestSummary {
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
}
```

Implementation approach:
- Find the `test result:` line with regex `test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored`
- Find the `failures:` section to extract failure details
- For each `---- <test_name> stdout ----` block, extract:
  - Test name
  - Panic location from `panicked at <file>:<line>:<col>:`
  - The message (lines between the panic location and the next `----` or `failures:` marker)
- Assign `F{n}` IDs sequentially

**Step 5: Run tests to verify they pass**

Run: `cargo test`
Expected: All test parser tests pass

**Step 6: Commit**

```bash
git add src/parser.rs tests/fixtures/test_stderr_*.txt
git commit -m "feat: test result stderr parser"
```

---

### Task 6: Plain Text Formatter

**Files:**
- Create: `src/format.rs`
- Create: `src/format/plain.rs`

**Step 1: Define the formatter trait**

`src/format.rs`:
```rust
mod plain;

pub use plain::PlainFormatter;

use crate::cli::Verbosity;
use crate::diagnostic::{Diagnostic, TestResult, RunSummary};

pub trait Formatter {
    fn format_diagnostic(&self, diag: &Diagnostic) -> String;
    fn format_test_failure(&self, result: &TestResult) -> String;
    fn format_summary(&self, summary: &RunSummary) -> String;
}
```

**Step 2: Write tests for plain text formatting**

In `src/format/plain.rs`:
```rust
use crate::cli::Verbosity;
use crate::diagnostic::{Diagnostic, DiagnosticLevel, TestResult, RunSummary};
use super::Formatter;

pub struct PlainFormatter {
    pub verbosity: Verbosity,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_warning() -> Diagnostic {
        Diagnostic {
            id: "W1".into(),
            level: DiagnosticLevel::Warning,
            code: Some("clippy::needless_return".into()),
            message: "unnecessary `return`".into(),
            file: Some("src/main.rs".into()),
            line: Some(42),
            col: Some(5),
            span_text: Some("    return Ok(value);".into()),
            span_label: Some("help: remove `return`: `Ok(value)`".into()),
            rendered: Some("warning: unnecessary...\n  --> src/main.rs:42:5\n...".into()),
            raw_json: None,
        }
    }

    #[test]
    fn terse_diagnostic() {
        let f = PlainFormatter { verbosity: Verbosity::Terse };
        let out = f.format_diagnostic(&sample_warning());
        assert_eq!(out, "W1 warning[clippy::needless_return] src/main.rs:42:5 unnecessary `return`");
    }

    #[test]
    fn verbose_diagnostic_includes_span() {
        let f = PlainFormatter { verbosity: Verbosity::Verbose };
        let out = f.format_diagnostic(&sample_warning());
        assert!(out.contains("W1 warning[clippy::needless_return] src/main.rs:42:5"));
        assert!(out.contains("return Ok(value)"));
        assert!(out.contains("help: remove `return`"));
    }

    #[test]
    fn very_verbose_uses_rendered() {
        let f = PlainFormatter { verbosity: Verbosity::VeryVerbose };
        let out = f.format_diagnostic(&sample_warning());
        assert!(out.contains("warning: unnecessary..."));
    }

    #[test]
    fn success_summary() {
        let f = PlainFormatter { verbosity: Verbosity::Terse };
        let summary = RunSummary {
            command: "clippy".into(),
            success: true,
            errors: 0,
            warnings: 0,
            tests_passed: 0,
            tests_failed: 0,
            tests_ignored: 0,
            elapsed_secs: 4.2,
        };
        let out = f.format_summary(&summary);
        assert_eq!(out, "ok (clippy) 0 warnings 4.2s");
    }

    #[test]
    fn test_summary_with_counts() {
        let f = PlainFormatter { verbosity: Verbosity::Terse };
        let summary = RunSummary {
            command: "test".into(),
            success: true,
            errors: 0,
            warnings: 0,
            tests_passed: 47,
            tests_failed: 0,
            tests_ignored: 0,
            elapsed_secs: 8.1,
        };
        let out = f.format_summary(&summary);
        assert_eq!(out, "ok (test) 47 passed, 0 failed 8.1s");
    }

    #[test]
    fn failure_summary() {
        let f = PlainFormatter { verbosity: Verbosity::Terse };
        let summary = RunSummary {
            command: "clippy".into(),
            success: false,
            errors: 1,
            warnings: 2,
            tests_passed: 0,
            tests_failed: 0,
            tests_ignored: 0,
            elapsed_secs: 4.2,
        };
        let out = f.format_summary(&summary);
        assert_eq!(out, "2 warnings, 1 error 4.2s");
    }

    #[test]
    fn test_failure_terse() {
        let f = PlainFormatter { verbosity: Verbosity::Terse };
        let tr = TestResult {
            id: "F1".into(),
            name: "tests::parse_config".into(),
            passed: false,
            failure_message: Some("assertion failed".into()),
            file: Some("src/config.rs".into()),
            line: Some(156),
        };
        let out = f.format_test_failure(&tr);
        assert_eq!(out, "F1 FAILED tests::parse_config");
    }

    #[test]
    fn test_failure_verbose() {
        let f = PlainFormatter { verbosity: Verbosity::Verbose };
        let tr = TestResult {
            id: "F1".into(),
            name: "tests::parse_config".into(),
            passed: false,
            failure_message: Some("assertion `left == right` failed\n  left: None\n right: Some(\"default\")".into()),
            file: Some("src/config.rs".into()),
            line: Some(156),
        };
        let out = f.format_test_failure(&tr);
        assert!(out.contains("F1 FAILED tests::parse_config"));
        assert!(out.contains("left == right"));
        assert!(out.contains("at src/config.rs:156"));
    }
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test`
Expected: FAIL

**Step 4: Implement PlainFormatter**

Implement `format_diagnostic`, `format_test_failure`, and `format_summary` per the design doc output format.

**Step 5: Run tests to verify they pass**

Run: `cargo test`
Expected: All formatter tests pass

**Step 6: Commit**

```bash
git add src/format.rs src/format/
git commit -m "feat: plain text formatter"
```

---

### Task 7: JSON Formatter

**Files:**
- Create: `src/format/json.rs`
- Modify: `src/format.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_diagnostic_output() {
        let f = JsonFormatter;
        let diag = /* same sample_warning as above */;
        let out = f.format_diagnostic(&diag);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["id"], "W1");
        assert_eq!(v["level"], "warning");
        assert_eq!(v["code"], "clippy::needless_return");
        assert_eq!(v["file"], "src/main.rs");
        assert_eq!(v["line"], 42);
        assert_eq!(v["col"], 5);
        assert_eq!(v["message"], "unnecessary `return`");
    }

    #[test]
    fn json_summary_output() {
        let f = JsonFormatter;
        let summary = /* sample summary */;
        let out = f.format_summary(&summary);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["summary"], true);
        assert_eq!(v["command"], "clippy");
    }
}
```

**Step 2: Run tests, verify fail**

**Step 3: Implement JsonFormatter**

Output compact single-line JSON per diagnostic using `serde_json::to_string`.

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git add src/format/json.rs src/format.rs
git commit -m "feat: JSON formatter"
```

---

### Task 8: TOON Formatter

**Files:**
- Create: `src/format/toon.rs`
- Modify: `src/format.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toon_formats_diagnostics_as_table() {
        let f = ToonFormatter;
        let diags = vec![sample_warning(), sample_error()];
        let out = f.format_diagnostics_table(&diags);
        // TOON table header declared once, rows follow
        assert!(out.starts_with("diagnostics\n"));
        assert!(out.contains("id level code file line col message\n"));
        assert!(out.contains("W1 warning clippy::needless_return src/main.rs 42 5"));
    }
}
```

**Step 2: Run tests, verify fail**

**Step 3: Implement ToonFormatter**

Use the `toon-format` crate. Build a `serde_json::Value` array of diagnostic objects and pass to `toon_format::encode()`. The crate should handle the tabular layout for uniform arrays.

If the crate API doesn't produce the exact table format we want, construct it manually — TOON tables are a simple format:
```
<table_name>
  <field1> <field2> <field3>
  <val1> <val2> <val3>
```

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git add src/format/toon.rs src/format.rs
git commit -m "feat: TOON formatter"
```

---

### Task 9: Cargo Runner

**Files:**
- Create: `src/runner.rs`

This is the core orchestration — spawning cargo and wiring up parsing.

**Step 1: Write the runner**

```rust
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::cli::{OutputFormat, Verbosity};
use crate::diagnostic::{Diagnostic, RunResult, RunSummary, TestResult};
use crate::format::{self, Formatter};
use crate::parser;

pub fn run_cargo(
    cargo_cmd: &str,
    cargo_args: &[OsString],
    format: &OutputFormat,
    verbosity: &Verbosity,
    no_cache: bool,
) -> i32 {
    let start = Instant::now();
    let formatter = format::create_formatter(format, verbosity);

    // Build the cargo command
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut cmd = Command::new(&cargo);
    cmd.arg(cargo_cmd);

    // fmt doesn't support --message-format=json
    let use_json = cargo_cmd != "fmt";
    if use_json {
        cmd.arg("--message-format=json");
    }

    cmd.args(cargo_args);

    if use_json {
        cmd.stdout(Stdio::piped());
    }
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cargo-terse: failed to spawn cargo: {e}");
            return 101;
        }
    };

    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut warning_id = 1usize;
    let mut error_id = 1usize;

    // Parse JSON from stdout (for build/check/clippy/test)
    if use_json {
        if let Some(stdout) = child.stdout.take() {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => continue,
                };
                if let Some(diag) = parser::parse_cargo_json_line(&line, &mut warning_id, &mut error_id) {
                    // Stream the formatted output immediately
                    println!("{}", formatter.format_diagnostic(&diag));
                    diagnostics.push(diag);
                }
            }
        }
    }

    // Capture stderr for test results
    let stderr_output = if let Some(stderr) = child.stderr.take() {
        let mut buf = String::new();
        BufReader::new(stderr).read_to_string(&mut buf).ok();
        buf
    } else {
        String::new()
    };

    let status = child.wait().unwrap_or_else(|_| std::process::exit(101));
    let elapsed = start.elapsed().as_secs_f64();

    // Parse test results from stderr
    let (test_results, test_summary) = if cargo_cmd == "test" {
        let (results, summary) = parser::parse_test_stderr(&stderr_output);
        for r in &results {
            if !r.passed {
                println!("{}", formatter.format_test_failure(r));
            }
        }
        (results, Some(summary))
    } else {
        (vec![], None)
    };

    // Build summary
    let summary = RunSummary {
        command: cargo_cmd.to_string(),
        success: status.success(),
        errors: diagnostics.iter().filter(|d| d.level == crate::diagnostic::DiagnosticLevel::Error).count(),
        warnings: diagnostics.iter().filter(|d| d.level == crate::diagnostic::DiagnosticLevel::Warning).count(),
        tests_passed: test_summary.as_ref().map_or(0, |s| s.passed),
        tests_failed: test_summary.as_ref().map_or(0, |s| s.failed),
        tests_ignored: test_summary.as_ref().map_or(0, |s| s.ignored),
        elapsed_secs: (elapsed * 10.0).round() / 10.0,
    };

    println!("{}", formatter.format_summary(&summary));

    // Write cache
    if !no_cache {
        let result = RunResult { diagnostics, test_results, summary };
        write_cache(&result);
    }

    status.code().unwrap_or(101)
}
```

Note: The `read_to_string` on stderr needs `use std::io::Read;`. Also, for test commands, stderr must be consumed concurrently with stdout to avoid deadlock — use `std::thread::spawn` to read stderr in a separate thread while the main thread reads stdout. Refine this during implementation.

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/runner.rs
git commit -m "feat: cargo runner with streaming output"
```

---

### Task 10: Drill-Down Cache

**Files:**
- Create: `src/cache.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_and_read_cache() {
        let dir = tempdir().unwrap();
        let result = RunResult { /* sample data with 2 diagnostics */ };
        write_cache_to(dir.path(), &result);
        let cached = read_cache_from(dir.path()).unwrap();
        assert_eq!(cached.diagnostics.len(), 2);
    }

    #[test]
    fn lookup_by_id() {
        let dir = tempdir().unwrap();
        let result = RunResult { /* sample data */ };
        write_cache_to(dir.path(), &result);
        let diag = lookup_diagnostic(dir.path(), "W1").unwrap();
        assert_eq!(diag.id, "W1");
        assert!(diag.rendered.is_some());
    }

    #[test]
    fn lookup_missing_id() {
        let dir = tempdir().unwrap();
        let result = RunResult { /* sample data */ };
        write_cache_to(dir.path(), &result);
        assert!(lookup_diagnostic(dir.path(), "E99").is_none());
    }
}
```

Add `tempfile` as a dev-dependency in Cargo.toml:
```toml
[dev-dependencies]
tempfile = "3"
```

**Step 2: Run tests, verify fail**

**Step 3: Implement cache**

```rust
use std::path::Path;
use crate::diagnostic::RunResult;

const CACHE_FILENAME: &str = ".terse-cache.json";

pub fn cache_dir() -> std::path::PathBuf {
    // Use CARGO_TARGET_DIR if set, otherwise "target"
    std::env::var("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("target"))
}

pub fn write_cache(result: &RunResult) {
    write_cache_to(&cache_dir(), result);
}

pub fn write_cache_to(dir: &Path, result: &RunResult) {
    let path = dir.join(CACHE_FILENAME);
    if let Ok(json) = serde_json::to_string(result) {
        let _ = std::fs::write(path, json);
    }
}

pub fn lookup_diagnostic(dir: &Path, id: &str) -> Option<crate::diagnostic::Diagnostic> {
    let path = dir.join(CACHE_FILENAME);
    let data = std::fs::read_to_string(path).ok()?;
    let result: RunResult = serde_json::from_str(&data).ok()?;
    result.diagnostics.into_iter()
        .chain_test_results(result.test_results)
        .find(|d| d.id == id)
}
```

Note: `lookup_diagnostic` needs to search both diagnostics and test_results. Refine the API during implementation — may need a common `CacheEntry` enum or just search both vecs.

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git add src/cache.rs Cargo.toml
git commit -m "feat: drill-down cache (write + lookup)"
```

---

### Task 11: Wire Up main.rs + Detail Command

**Files:**
- Modify: `src/main.rs`

**Step 1: Wire everything together**

```rust
mod cache;
mod cli;
mod diagnostic;
mod format;
mod parser;
mod runner;

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    let cmd = match cli::parse_args(args) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!("cargo-terse: {e}");
            std::process::exit(2);
        }
    };

    match cmd {
        cli::Command::Run { cargo_cmd, format, verbosity, no_cache, cargo_args } => {
            let code = runner::run_cargo(&cargo_cmd, &cargo_args, &format, &verbosity, no_cache);
            std::process::exit(code);
        }
        cli::Command::Detail { id, format } => {
            let dir = cache::cache_dir();
            match cache::lookup_diagnostic(&dir, &id) {
                Some(entry) => {
                    let formatter = format::create_formatter(&format, &cli::Verbosity::VeryVerbose);
                    println!("{}", formatter.format_diagnostic(&entry));
                }
                None => {
                    eprintln!("cargo-terse: no cached diagnostic with id '{id}'");
                    eprintln!("hint: run a cargo terse command first, then use detail");
                    std::process::exit(1);
                }
            }
        }
    }
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up main with run and detail commands"
```

---

### Task 12: fmt Handling

**Files:**
- Create: `src/fmt.rs`

**Step 1: Write tests**

Test the diff-output parser that extracts filenames from `cargo fmt --check` output.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_fmt_diff_filenames() {
        let diff = "Diff in /path/to/src/main.rs at line 42:\n ...\nDiff in /path/to/src/lib.rs at line 10:\n ...";
        let files = parse_fmt_diff(diff);
        assert_eq!(files, vec!["src/main.rs", "src/lib.rs"]);
    }

    #[test]
    fn parse_fmt_no_diff() {
        let files = parse_fmt_diff("");
        assert!(files.is_empty());
    }
}
```

**Step 2: Run tests, verify fail**

**Step 3: Implement fmt handling**

The runner already handles `fmt` specially (no `--message-format=json`). Add `--check` flag when running fmt, parse the diff output for filenames, and format accordingly.

**Step 4: Run tests, verify pass**

**Step 5: Commit**

```bash
git add src/fmt.rs
git commit -m "feat: fmt --check output parsing"
```

---

### Task 13: Integration Test — End-to-End

**Files:**
- Create: `tests/integration.rs`
- Create: `tests/fixtures/sample_project/` (a tiny Rust project with known warnings/errors)

**Step 1: Create a sample project fixture**

Create a minimal Rust project in `tests/fixtures/sample_project/` with:
- A `Cargo.toml` with package name `sample`
- `src/lib.rs` with one unused variable (warning) and one function

**Step 2: Write integration tests**

```rust
use std::process::Command;

fn cargo_terse() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-terse"));
    cmd.arg("terse");
    cmd
}

#[test]
fn check_reports_warning_concisely() {
    let output = cargo_terse()
        .arg("check")
        .current_dir("tests/fixtures/sample_project")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should contain a terse warning line
    assert!(stdout.contains("W1 warning"));
    // Should NOT contain "Compiling" progress
    assert!(!stdout.contains("Compiling"));
}

#[test]
fn json_format_outputs_valid_jsonl() {
    let output = cargo_terse()
        .args(["--format", "json", "check"])
        .current_dir("tests/fixtures/sample_project")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for line in stdout.lines() {
        // Every line should be valid JSON
        serde_json::from_str::<serde_json::Value>(line).unwrap();
    }
}

#[test]
fn detail_command_works_after_run() {
    // First run check to populate cache
    cargo_terse()
        .arg("check")
        .current_dir("tests/fixtures/sample_project")
        .output()
        .unwrap();

    // Then detail a diagnostic
    let output = cargo_terse()
        .args(["detail", "W1"])
        .current_dir("tests/fixtures/sample_project")
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.is_empty());
}

#[test]
fn exit_code_mirrors_cargo() {
    let output = cargo_terse()
        .arg("check")
        .current_dir("tests/fixtures/sample_project")
        .output()
        .unwrap();
    // Project has only warnings, so exit code should be 0
    assert!(output.status.success());
}
```

**Step 3: Run integration tests**

Run: `cargo test --test integration`
Expected: All pass

**Step 4: Commit**

```bash
git add tests/
git commit -m "test: end-to-end integration tests"
```

---

### Task 14: Polish + README

**Files:**
- Create: `README.md`
- Modify: `Cargo.toml` (add metadata for crates.io)

**Step 1: Write README**

Cover: what it does, install (`cargo install cargo-terse`), usage examples, output format examples, the detail drill-down feature.

**Step 2: Add crates.io metadata**

Add `repository`, `keywords`, `categories` to Cargo.toml.

**Step 3: Run full test suite one final time**

Run: `cargo test`
Expected: All tests pass

Run: `cargo clippy`
Expected: No warnings

**Step 4: Commit**

```bash
git add README.md Cargo.toml
git commit -m "docs: README and crates.io metadata"
```

---

## Dependency Summary

```toml
[dependencies]
lexopt = "0.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toon-format = "0.2"

[dev-dependencies]
tempfile = "3"
```

## File Structure

```
cargo-terse/
├── Cargo.toml
├── README.md
├── docs/plans/
│   ├── 2026-02-21-cargo-terse-design.md
│   └── 2026-02-21-cargo-terse-implementation.md
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── diagnostic.rs
│   ├── parser.rs
│   ├── runner.rs
│   ├── cache.rs
│   ├── fmt.rs
│   └── format/
│       ├── mod.rs
│       ├── plain.rs
│       ├── json.rs
│       └── toon.rs
└── tests/
    ├── integration.rs
    └── fixtures/
        ├── clippy_output.json
        ├── test_stderr_pass.txt
        ├── test_stderr_fail.txt
        └── sample_project/
            ├── Cargo.toml
            └── src/lib.rs
```
