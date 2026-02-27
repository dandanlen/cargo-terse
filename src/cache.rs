use std::path::{Path, PathBuf};

use crate::diagnostic::{Diagnostic, RunResult, TestResult};

const CACHE_FILENAME: &str = ".terse-cache.json";

pub fn cache_dir() -> PathBuf {
    std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("target"))
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

pub fn lookup_diagnostic(id: &str) -> Option<Diagnostic> {
    lookup_diagnostic_from(&cache_dir(), id)
}

pub fn lookup_diagnostic_from(dir: &Path, id: &str) -> Option<Diagnostic> {
    let path = dir.join(CACHE_FILENAME);
    let data = std::fs::read_to_string(path).ok()?;
    let result: RunResult = serde_json::from_str(&data).ok()?;
    result.diagnostics.into_iter().find(|d| d.id == id)
}

pub fn lookup_test_result(id: &str) -> Option<TestResult> {
    lookup_test_result_from(&cache_dir(), id)
}

pub fn lookup_test_result_from(dir: &Path, id: &str) -> Option<TestResult> {
    let path = dir.join(CACHE_FILENAME);
    let data = std::fs::read_to_string(path).ok()?;
    let result: RunResult = serde_json::from_str(&data).ok()?;
    result.test_results.into_iter().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{DiagnosticLevel, RunSummary, TestResult};

    fn make_result() -> RunResult {
        RunResult {
            diagnostics: vec![
                Diagnostic {
                    id: "W-a1b2".to_string(),
                    level: DiagnosticLevel::Warning,
                    code: Some("dead_code".to_string()),
                    message: "unused variable `x`".to_string(),
                    file: Some("src/main.rs".to_string()),
                    line: Some(5),
                    col: Some(9),
                    span_text: None,
                    span_label: None,
                    rendered: None,
                    raw_json: None,
                },
                Diagnostic {
                    id: "E-c3d4".to_string(),
                    level: DiagnosticLevel::Error,
                    code: Some("E0308".to_string()),
                    message: "mismatched types".to_string(),
                    file: Some("src/lib.rs".to_string()),
                    line: Some(10),
                    col: Some(5),
                    span_text: None,
                    span_label: None,
                    rendered: None,
                    raw_json: None,
                },
            ],
            test_results: vec![TestResult {
                id: "F-e5f6".to_string(),
                name: "tests::my_test".to_string(),
                passed: false,
                failure_message: Some("assertion failed".to_string()),
                file: None,
                line: None,
            }],
            summary: RunSummary {
                command: "check".to_string(),
                success: false,
                errors: 1,
                warnings: 1,
                tests_passed: 0,
                tests_failed: 1,
                tests_ignored: 0,
                elapsed_secs: 1.23,
                raw_bytes: 0,
                output_bytes: 0,
            },
        }
    }

    #[test]
    fn write_and_read_cache() {
        let dir = tempfile::tempdir().unwrap();
        let result = make_result();
        write_cache_to(dir.path(), &result);

        let data = std::fs::read_to_string(dir.path().join(CACHE_FILENAME)).unwrap();
        let loaded: RunResult = serde_json::from_str(&data).unwrap();

        assert_eq!(loaded.diagnostics.len(), 2);
        assert_eq!(loaded.diagnostics[0].id, "W-a1b2");
        assert_eq!(loaded.diagnostics[1].id, "E-c3d4");
        assert_eq!(loaded.test_results.len(), 1);
        assert_eq!(loaded.test_results[0].name, "tests::my_test");
        assert_eq!(loaded.summary.command, "check");
        assert!(!loaded.summary.success);
    }

    #[test]
    fn lookup_by_id() {
        let dir = tempfile::tempdir().unwrap();
        write_cache_to(dir.path(), &make_result());

        let diag = lookup_diagnostic_from(dir.path(), "W-a1b2").unwrap();
        assert_eq!(diag.id, "W-a1b2");
        assert_eq!(diag.message, "unused variable `x`");
        assert_eq!(diag.level, DiagnosticLevel::Warning);
    }

    #[test]
    fn lookup_missing_id() {
        let dir = tempfile::tempdir().unwrap();
        write_cache_to(dir.path(), &make_result());

        assert!(lookup_diagnostic_from(dir.path(), "W-9999").is_none());
    }

    #[test]
    fn lookup_test_result_by_id() {
        let dir = tempfile::tempdir().unwrap();
        write_cache_to(dir.path(), &make_result());

        let test = lookup_test_result_from(dir.path(), "F-e5f6").unwrap();
        assert_eq!(test.id, "F-e5f6");
        assert_eq!(test.name, "tests::my_test");
        assert!(!test.passed);
    }

    #[test]
    fn lookup_test_result_missing() {
        let dir = tempfile::tempdir().unwrap();
        write_cache_to(dir.path(), &make_result());

        assert!(lookup_test_result_from(dir.path(), "F-9999").is_none());
    }
}
