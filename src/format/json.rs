use crate::diagnostic::{Diagnostic, DiagnosticLevel, RunSummary, TestResult};
use crate::format::Formatter;

pub struct JsonFormatter;

impl Formatter for JsonFormatter {
    fn format_diagnostic(&self, diag: &Diagnostic) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert("id".into(), diag.id.clone().into());
        obj.insert(
            "level".into(),
            match diag.level {
                DiagnosticLevel::Error => "error",
                DiagnosticLevel::Warning => "warning",
                DiagnosticLevel::Note => "note",
                DiagnosticLevel::Help => "help",
            }
            .into(),
        );
        if let Some(code) = &diag.code {
            obj.insert("code".into(), code.clone().into());
        }
        if let Some(file) = &diag.file {
            obj.insert("file".into(), file.clone().into());
        }
        if let Some(line) = diag.line {
            obj.insert("line".into(), line.into());
        }
        if let Some(col) = diag.col {
            obj.insert("col".into(), col.into());
        }
        obj.insert("message".into(), diag.message.clone().into());
        serde_json::to_string(&obj).unwrap()
    }

    fn format_test_failure(&self, result: &TestResult) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert("id".into(), result.id.clone().into());
        obj.insert("level".into(), "fail".into());
        obj.insert("test".into(), result.name.clone().into());
        if let Some(msg) = &result.failure_message {
            obj.insert("message".into(), msg.clone().into());
        }
        if let Some(file) = &result.file {
            obj.insert("file".into(), file.clone().into());
        }
        if let Some(line) = result.line {
            obj.insert("line".into(), line.into());
        }
        serde_json::to_string(&obj).unwrap()
    }

    fn format_summary(&self, summary: &RunSummary) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert("summary".into(), true.into());
        obj.insert("command".into(), summary.command.clone().into());
        obj.insert(
            "status".into(),
            if summary.success { "ok" } else { "fail" }.into(),
        );
        obj.insert("warnings".into(), summary.warnings.into());
        obj.insert("errors".into(), summary.errors.into());
        if summary.command == "test" {
            obj.insert("tests_passed".into(), summary.tests_passed.into());
            obj.insert("tests_failed".into(), summary.tests_failed.into());
        }
        obj.insert(
            "elapsed_secs".into(),
            serde_json::Number::from_f64(summary.elapsed_secs)
                .unwrap_or_else(|| serde_json::Number::from(0))
                .into(),
        );
        serde_json::to_string(&obj).unwrap()
    }
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
            span_text: None,
            span_label: None,
            rendered: None,
            raw_json: None,
        }
    }

    #[test]
    fn json_diagnostic_is_valid() {
        let out = JsonFormatter.format_diagnostic(&sample_warning());
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
    fn json_test_failure() {
        let r = TestResult {
            id: "F1".into(),
            name: "tests::parse_config".into(),
            passed: false,
            failure_message: Some("assertion failed".into()),
            file: Some("src/config.rs".into()),
            line: Some(156),
        };
        let out = JsonFormatter.format_test_failure(&r);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["id"], "F1");
        assert_eq!(v["level"], "fail");
        assert_eq!(v["test"], "tests::parse_config");
        assert_eq!(v["file"], "src/config.rs");
        assert_eq!(v["line"], 156);
    }

    #[test]
    fn json_summary() {
        let s = RunSummary {
            command: "clippy".into(),
            success: false,
            errors: 1,
            warnings: 2,
            tests_passed: 0,
            tests_failed: 0,
            tests_ignored: 0,
            elapsed_secs: 4.2,
        };
        let out = JsonFormatter.format_summary(&s);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["summary"], true);
        assert_eq!(v["command"], "clippy");
        assert_eq!(v["status"], "fail");
        assert_eq!(v["warnings"], 2);
        assert_eq!(v["errors"], 1);
    }
}
