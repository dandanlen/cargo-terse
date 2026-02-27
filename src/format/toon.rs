use crate::diagnostic::{Diagnostic, DiagnosticLevel, RunSummary, TestResult};
use crate::format::Formatter;

pub struct ToonFormatter;

impl ToonFormatter {
    fn diagnostic_to_json(diag: &Diagnostic) -> serde_json::Value {
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
        obj.insert("code".into(), diag.code.clone().unwrap_or_default().into());
        obj.insert("file".into(), diag.file.clone().unwrap_or_default().into());
        obj.insert("line".into(), diag.line.unwrap_or(0).into());
        obj.insert("col".into(), diag.col.unwrap_or(0).into());
        obj.insert("message".into(), diag.message.clone().into());
        serde_json::Value::Object(obj)
    }
}

impl Formatter for ToonFormatter {
    fn format_diagnostic(&self, diag: &Diagnostic) -> String {
        let obj = Self::diagnostic_to_json(diag);
        toon_format::encode_default(&obj).unwrap_or_else(|_| "encoding error".into())
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
        toon_format::encode_default(serde_json::Value::Object(obj))
            .unwrap_or_else(|_| "encoding error".into())
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
        obj.insert(
            "elapsed_secs".into(),
            serde_json::Number::from_f64(summary.elapsed_secs)
                .unwrap_or_else(|| serde_json::Number::from(0))
                .into(),
        );
        if summary.raw_bytes > 0 {
            obj.insert("raw_bytes".into(), summary.raw_bytes.into());
            obj.insert("output_bytes".into(), summary.output_bytes.into());
        }
        toon_format::encode_default(serde_json::Value::Object(obj))
            .unwrap_or_else(|_| "encoding error".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_warning() -> Diagnostic {
        Diagnostic {
            id: "W-98bf".into(),
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
    fn toon_single_diagnostic_encodes() {
        let out = ToonFormatter.format_diagnostic(&sample_warning());
        // TOON quotes strings containing special characters like hyphens
        assert!(out.contains("id: \"W-98bf\""));
        assert!(out.contains("level: warning"));
        assert!(out.contains("clippy::needless_return"));
    }

    #[test]
    fn toon_test_failure_encodes() {
        let r = TestResult {
            id: "F-c382".into(),
            name: "tests::parse_config".into(),
            passed: false,
            failure_message: Some("assertion failed".into()),
            file: Some("src/config.rs".into()),
            line: Some(156),
        };
        let out = ToonFormatter.format_test_failure(&r);
        assert!(out.contains("id: \"F-c382\""));
        assert!(out.contains("level: fail"));
        assert!(out.contains("tests::parse_config"));
    }

    #[test]
    fn toon_summary_encodes() {
        let s = RunSummary {
            command: "clippy".into(),
            success: false,
            errors: 1,
            warnings: 2,
            tests_passed: 0,
            tests_failed: 0,
            tests_ignored: 0,
            elapsed_secs: 4.2,
            raw_bytes: 0,
            output_bytes: 0,
        };
        let out = ToonFormatter.format_summary(&s);
        assert!(out.contains("summary: true"));
        assert!(out.contains("command: clippy"));
        assert!(out.contains("status: fail"));
    }
}

