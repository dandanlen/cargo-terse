use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::diagnostic::{Diagnostic, DiagnosticLevel, TestResult};

/// Generates a stable content-hash ID with the given prefix character.
///
/// Uses file + error code + message as hash inputs so IDs are robust to line number
/// changes from edits. Starts at 4 hex digits and extends on collision.
fn unique_id(
    prefix: char,
    file: Option<&str>,
    code: Option<&str>,
    message: &str,
    used: &mut HashSet<String>,
) -> String {
    let mut hasher = DefaultHasher::new();
    file.unwrap_or("").hash(&mut hasher);
    code.unwrap_or("").hash(&mut hasher);
    message.hash(&mut hasher);
    let h = hasher.finish();
    for width in 4..=16 {
        let mask = (1u64 << (width * 4)) - 1;
        let id = format!("{}-{:0width$x}", prefix, h & mask, width = width);
        if used.insert(id.clone()) {
            return id;
        }
    }
    format!("{}-{:016x}", prefix, h) // fallback: full hash
}

pub fn parse_cargo_json_line(line: &str, used_ids: &mut HashSet<String>) -> Option<Diagnostic> {
    if !line.starts_with('{') {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    if v["reason"] != "compiler-message" {
        return None;
    }
    let msg = &v["message"];
    let level = match msg["level"].as_str()? {
        "error" => DiagnosticLevel::Error,
        "warning" => DiagnosticLevel::Warning,
        _ => return None,
    };
    let primary = msg["spans"]
        .as_array()?
        .iter()
        .find(|s| s["is_primary"] == true)?;

    let prefix = match level {
        DiagnosticLevel::Error => 'E',
        DiagnosticLevel::Warning => 'W',
        // Note and Help are filtered out above (line 43-46).
        DiagnosticLevel::Note | DiagnosticLevel::Help => unreachable!(),
    };
    let file = primary["file_name"].as_str();
    let code = msg["code"]["code"].as_str();
    let message = msg["message"].as_str()?;
    let id = unique_id(prefix, file, code, message, used_ids);

    Some(Diagnostic {
        id,
        level,
        code: code.map(str::to_owned),
        message: message.to_owned(),
        file: file.map(str::to_owned),
        line: primary["line_start"].as_u64().map(|n| n as usize),
        col: primary["column_start"].as_u64().map(|n| n as usize),
        span_text: primary["text"][0]["text"].as_str().map(str::to_owned),
        span_label: primary["label"].as_str().map(str::to_owned),
        rendered: msg["rendered"].as_str().map(str::to_owned),
        raw_json: Some(v),
    })
}

pub struct TestSummary {
    pub passed: usize,
    pub failed: usize,
    pub ignored: usize,
}

/// Parses libtest stderr output, returning only failed tests and the summary counts.
///
/// Failures are extracted from the `failures:` block where each entry starts with
/// `---- <name> stdout ----`. Panic location and message are parsed from the block body.
pub fn parse_test_stderr(stderr: &str) -> (Vec<TestResult>, TestSummary) {
    // Find the summary line; if absent (e.g. compilation failed before tests ran), bail early.
    let summary_line = match stderr.lines().find(|l| l.starts_with("test result:")) {
        Some(l) => l,
        None => {
            return (
                vec![],
                TestSummary {
                    passed: 0,
                    failed: 0,
                    ignored: 0,
                },
            )
        }
    };

    let summary = parse_summary_line(summary_line);

    if summary.failed == 0 {
        return (vec![], summary);
    }

    // The failures detail block sits between the first `failures:` line and the second one
    // (which lists test names only). We split on `failures:\n` and take the middle chunk.
    let results = parse_failure_blocks(stderr);
    (results, summary)
}

fn parse_summary_line(line: &str) -> TestSummary {
    // Format: `test result: ok/FAILED. N passed; N failed; N ignored; ...`
    // Segments separated by `;` are like "test result: ok. 3 passed", "2 failed", "0 ignored".
    // Use rsplit on each segment to grab the number just before the trailing keyword.
    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;
    for segment in line.split(';') {
        let s = segment.trim();
        if let Some(rest) = s.strip_suffix(" passed") {
            if let Ok(n) = rest.rsplit(' ').next().unwrap_or("").parse() {
                passed = n;
            }
        } else if let Some(rest) = s.strip_suffix(" failed") {
            if let Ok(n) = rest.rsplit(' ').next().unwrap_or("").parse() {
                failed = n;
            }
        } else if let Some(rest) = s.strip_suffix(" ignored") {
            if let Ok(n) = rest.rsplit(' ').next().unwrap_or("").parse() {
                ignored = n;
            }
        }
    }
    TestSummary {
        passed,
        failed,
        ignored,
    }
}

fn parse_failure_blocks(stderr: &str) -> Vec<TestResult> {
    // Locate the first `failures:` section header.
    let failures_marker = "failures:\n";
    let first = match stderr.find(failures_marker) {
        Some(i) => i + failures_marker.len(),
        None => return vec![],
    };
    // The second `failures:` block (name list) begins after the detail blocks.
    let detail_section = match stderr[first..].find(failures_marker) {
        Some(i) => &stderr[first..first + i],
        None => &stderr[first..],
    };

    let mut results = Vec::new();
    let mut used_ids: HashSet<String> = HashSet::new();

    // Each failure block starts with `---- <name> stdout ----`
    for block in detail_section.split("\n---- ") {
        let block = block
            .trim_start_matches("---- ")
            .trim_start_matches('-')
            .trim_start();
        // After splitting on "\n---- ", the first chunk before any `----` is just whitespace.
        let header_end = match block.find(" stdout ----") {
            Some(i) => i,
            None => continue,
        };
        let name = block[..header_end].trim().to_owned();
        if name.is_empty() {
            continue;
        }
        let body = block[header_end + " stdout ----".len()..].trim();

        // Parse panic location: `thread '...' panicked at <file>:<line>:<col>:`
        let mut file = None;
        let mut line = None;
        let mut message_lines: Vec<&str> = Vec::new();
        let mut past_panic = false;

        for l in body.lines() {
            if !past_panic {
                if let Some(rest) = l.trim().strip_prefix("thread '") {
                    // thread '<name>' panicked at <file>:<line>:<col>:
                    if let Some(at_pos) = rest.find("' panicked at ") {
                        let location = &rest[at_pos + "' panicked at ".len()..];
                        // location may have a trailing `:` — strip it
                        let location = location.trim_end_matches(':');
                        // Split from the right: col, then line, then file
                        let mut parts = location.rsplitn(3, ':');
                        let _col = parts.next();
                        if let Some(ln) = parts.next().and_then(|n| n.parse().ok()) {
                            line = Some(ln);
                        }
                        if let Some(f) = parts.next() {
                            file = Some(f.to_owned());
                        }
                        past_panic = true;
                    }
                }
            } else {
                message_lines.push(l);
            }
        }

        let failure_message = if message_lines.is_empty() {
            None
        } else {
            Some(message_lines.join("\n").trim().to_owned())
        };

        let id = unique_id('F', None, None, &name, &mut used_ids);
        results.push(TestResult {
            id,
            name,
            passed: false,
            failure_message,
            file,
            line,
        });
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_lines() -> Vec<String> {
        include_str!("../tests/fixtures/clippy_output.json")
            .lines()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn ignores_compiler_artifact() {
        let lines = fixture_lines();
        assert!(parse_cargo_json_line(&lines[0], &mut HashSet::new()).is_none());
    }

    #[test]
    fn parses_warning() {
        let lines = fixture_lines();
        let d = parse_cargo_json_line(&lines[1], &mut HashSet::new()).expect("expected Some");
        assert!(d.id.starts_with("W-"), "expected W- prefix, got: {}", d.id);
        assert!(
            d.id.len() >= 6,
            "expected at least 4 hex chars after prefix, got: {}",
            d.id
        );
        assert_eq!(d.level, DiagnosticLevel::Warning);
        assert_eq!(d.code.as_deref(), Some("clippy::needless_return"));
        assert_eq!(d.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(d.line, Some(42));
        assert_eq!(d.col, Some(5));
        assert_eq!(d.message, "unnecessary `return` statement");
        assert!(d.rendered.is_some());
    }

    #[test]
    fn parses_error() {
        let lines = fixture_lines();
        let d = parse_cargo_json_line(&lines[2], &mut HashSet::new()).expect("expected Some");
        assert!(d.id.starts_with("E-"), "expected E- prefix, got: {}", d.id);
        assert!(
            d.id.len() >= 6,
            "expected at least 4 hex chars after prefix, got: {}",
            d.id
        );
        assert_eq!(d.level, DiagnosticLevel::Error);
        assert_eq!(d.code.as_deref(), Some("E0308"));
        assert_eq!(d.file.as_deref(), Some("src/handler.rs"));
        assert_eq!(d.line, Some(93));
        assert_eq!(d.col, Some(12));
    }

    #[test]
    fn ids_are_stable_across_calls() {
        // Same diagnostic content must always produce the same ID — the whole point of hashing.
        let lines = fixture_lines();
        let d1 = parse_cargo_json_line(&lines[1], &mut HashSet::new()).expect("expected Some");
        let d2 = parse_cargo_json_line(&lines[1], &mut HashSet::new()).expect("expected Some");
        assert_eq!(d1.id, d2.id, "same input must produce same ID");

        let e1 = parse_cargo_json_line(&lines[2], &mut HashSet::new()).expect("expected Some");
        let e2 = parse_cargo_json_line(&lines[2], &mut HashSet::new()).expect("expected Some");
        assert_eq!(e1.id, e2.id, "same input must produce same ID");
    }

    #[test]
    fn warning_and_error_have_distinct_ids() {
        let lines = fixture_lines();
        let mut used = HashSet::new();
        let w = parse_cargo_json_line(&lines[1], &mut used).expect("expected Some");
        let e = parse_cargo_json_line(&lines[2], &mut used).expect("expected Some");
        assert_ne!(w.id, e.id);
        assert!(w.id.starts_with("W-"));
        assert!(e.id.starts_with("E-"));
    }

    #[test]
    fn ignores_non_json() {
        assert!(parse_cargo_json_line("Compiling foo v0.1.0", &mut HashSet::new()).is_none());
        assert!(parse_cargo_json_line("", &mut HashSet::new()).is_none());
        assert!(parse_cargo_json_line("not json at all", &mut HashSet::new()).is_none());
    }

    #[test]
    fn ignores_build_finished() {
        let lines = fixture_lines();
        assert!(parse_cargo_json_line(&lines[3], &mut HashSet::new()).is_none());
    }

    // --- parse_test_stderr tests ---

    #[test]
    fn parse_passing_test_output() {
        let stderr = include_str!("../tests/fixtures/test_stderr_pass.txt");
        let (results, summary) = parse_test_stderr(stderr);
        assert!(results.is_empty());
        assert_eq!(summary.passed, 3);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.ignored, 0);
    }

    #[test]
    fn parse_failing_test_output() {
        let stderr = include_str!("../tests/fixtures/test_stderr_fail.txt");
        let (results, summary) = parse_test_stderr(stderr);
        assert_eq!(results.len(), 2);
        assert_eq!(summary.passed, 2);
        assert_eq!(summary.failed, 2);
        assert_eq!(summary.ignored, 0);
    }

    #[test]
    fn parse_empty_stderr() {
        let (results, summary) = parse_test_stderr("");
        assert!(results.is_empty());
        assert_eq!(summary.passed, 0);
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.ignored, 0);
    }

    #[test]
    fn parse_failing_f1_details() {
        let stderr = include_str!("../tests/fixtures/test_stderr_fail.txt");
        let (results, _) = parse_test_stderr(stderr);
        let f1 = &results[0];
        assert!(
            f1.id.starts_with("F-"),
            "expected F- prefix, got: {}",
            f1.id
        );
        assert_eq!(f1.name, "tests::parse_config_missing_field");
        assert_eq!(f1.file.as_deref(), Some("src/config.rs"));
        assert_eq!(f1.line, Some(156));
        assert!(f1
            .failure_message
            .as_deref()
            .unwrap_or("")
            .contains("left == right"));
        assert!(!f1.passed);
    }

    #[test]
    fn parse_failing_f2_details() {
        let stderr = include_str!("../tests/fixtures/test_stderr_fail.txt");
        let (results, _) = parse_test_stderr(stderr);
        let f2 = &results[1];
        assert!(
            f2.id.starts_with("F-"),
            "expected F- prefix, got: {}",
            f2.id
        );
        assert_eq!(f2.name, "tests::handler_timeout");
        assert_eq!(f2.file.as_deref(), Some("src/handler.rs"));
        assert_eq!(f2.line, Some(203));
        assert!(!f2.passed);
    }

    #[test]
    fn test_failure_ids_are_stable() {
        // Same test name must always produce the same F- ID.
        let stderr = include_str!("../tests/fixtures/test_stderr_fail.txt");
        let (results1, _) = parse_test_stderr(stderr);
        let (results2, _) = parse_test_stderr(stderr);
        assert_eq!(results1[0].id, results2[0].id);
        assert_eq!(results1[1].id, results2[1].id);
    }

    #[test]
    fn test_failure_ids_differ_by_name() {
        let stderr = include_str!("../tests/fixtures/test_stderr_fail.txt");
        let (results, _) = parse_test_stderr(stderr);
        assert_ne!(
            results[0].id, results[1].id,
            "different test names must produce different IDs"
        );
    }
}
