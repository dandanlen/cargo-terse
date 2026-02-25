use std::fmt::Write as _;

use crate::cli::Verbosity;
use crate::diagnostic::{Diagnostic, DiagnosticLevel, RunSummary, TestResult};
use crate::format::Formatter;

pub struct PlainFormatter {
    pub verbosity: Verbosity,
}

impl PlainFormatter {
    fn terse_diagnostic_line(diag: &Diagnostic) -> String {
        let level = match diag.level {
            DiagnosticLevel::Error => "error",
            DiagnosticLevel::Warning => "warning",
            DiagnosticLevel::Note => "note",
            DiagnosticLevel::Help => "help",
        };

        let kind = match &diag.code {
            Some(code) => format!("{level}[{code}]"),
            None => level.to_string(),
        };

        let location = match (&diag.file, diag.line, diag.col) {
            (Some(file), Some(line), Some(col)) => format!(" {file}:{line}:{col}"),
            (Some(file), Some(line), None) => format!(" {file}:{line}"),
            (Some(file), None, _) => format!(" {file}"),
            _ => String::new(),
        };

        format!("{} {kind}{location} {}", diag.id, diag.message)
    }
}

impl Formatter for PlainFormatter {
    fn format_diagnostic(&self, diag: &Diagnostic) -> String {
        match self.verbosity {
            Verbosity::VeryVerbose => {
                // Pass through rendered; fall back to terse if absent.
                match &diag.rendered {
                    Some(r) => r.clone(),
                    None => Self::terse_diagnostic_line(diag),
                }
            }
            Verbosity::Verbose => {
                let mut out = Self::terse_diagnostic_line(diag);
                if let Some(span) = &diag.span_text {
                    write!(out, "\n   |     {span}").unwrap();
                    if let Some(label) = &diag.span_label {
                        write!(out, "\n   |     {label}").unwrap();
                    }
                }
                out
            }
            Verbosity::Terse => Self::terse_diagnostic_line(diag),
        }
    }

    fn format_test_failure(&self, result: &TestResult) -> String {
        let first = format!("{} FAILED {}", result.id, result.name);
        if matches!(self.verbosity, Verbosity::Terse) {
            return first;
        }
        // Verbose and VeryVerbose: append failure message and location.
        let mut out = first;
        if let Some(msg) = &result.failure_message {
            for line in msg.lines() {
                write!(out, "\n   {line}").unwrap();
            }
        }
        if let Some(file) = &result.file {
            match result.line {
                Some(line) => write!(out, "\n   at {file}:{line}").unwrap(),
                None => write!(out, "\n   at {file}").unwrap(),
            }
        }
        out
    }

    fn format_summary(&self, summary: &RunSummary) -> String {
        let elapsed = format!("{:.1}s", summary.elapsed_secs);
        match summary.command.as_str() {
            "test" => {
                if summary.success {
                    format!(
                        "ok (test) {} passed, {} failed {}",
                        summary.tests_passed, summary.tests_failed, elapsed
                    )
                } else {
                    format!(
                        "test result: FAILED. {} passed; {} failed; {} ignored {}",
                        summary.tests_passed, summary.tests_failed, summary.tests_ignored, elapsed
                    )
                }
            }
            cmd => {
                if summary.success {
                    let w = summary.warnings;
                    format!(
                        "ok ({cmd}) {} {} {elapsed}",
                        w,
                        if w == 1 { "warning" } else { "warnings" }
                    )
                } else {
                    format!(
                        "{} {}, {} {} {elapsed}",
                        summary.warnings,
                        if summary.warnings == 1 {
                            "warning"
                        } else {
                            "warnings"
                        },
                        summary.errors,
                        if summary.errors == 1 {
                            "error"
                        } else {
                            "errors"
                        }
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diag(id: &str, level: DiagnosticLevel, code: Option<&str>, message: &str) -> Diagnostic {
        Diagnostic {
            id: id.to_string(),
            level,
            code: code.map(str::to_string),
            message: message.to_string(),
            file: Some("src/main.rs".to_string()),
            line: Some(42),
            col: Some(5),
            span_text: None,
            span_label: None,
            rendered: None,
            raw_json: None,
        }
    }

    fn summary(
        cmd: &str,
        success: bool,
        warnings: usize,
        errors: usize,
        elapsed: f64,
    ) -> RunSummary {
        RunSummary {
            command: cmd.to_string(),
            success,
            errors,
            warnings,
            tests_passed: 0,
            tests_failed: 0,
            tests_ignored: 0,
            elapsed_secs: elapsed,
        }
    }

    // 1. Terse diagnostic with code and location → one-line format
    #[test]
    fn terse_diagnostic() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Terse,
        };
        let d = diag(
            "W1",
            DiagnosticLevel::Warning,
            Some("clippy::needless_return"),
            "unnecessary `return`",
        );
        assert_eq!(
            fmt.format_diagnostic(&d),
            "W1 warning[clippy::needless_return] src/main.rs:42:5 unnecessary `return`"
        );
    }

    // 2. Terse diagnostic without code → no brackets
    #[test]
    fn terse_diagnostic_no_code() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Terse,
        };
        let d = diag("W1", DiagnosticLevel::Warning, None, "unused variable");
        assert_eq!(
            fmt.format_diagnostic(&d),
            "W1 warning src/main.rs:42:5 unused variable"
        );
    }

    // 3. Verbose diagnostic includes span_text and span_label
    #[test]
    fn verbose_diagnostic_includes_span() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Verbose,
        };
        let mut d = diag(
            "W1",
            DiagnosticLevel::Warning,
            Some("clippy::needless_return"),
            "unnecessary `return`",
        );
        d.span_text = Some("return Ok(value);".to_string());
        d.span_label = Some("^^^^^^^^^^^^^^^^^ help: remove `return`: `Ok(value)`".to_string());
        let out = fmt.format_diagnostic(&d);
        assert_eq!(
            out,
            "W1 warning[clippy::needless_return] src/main.rs:42:5 unnecessary `return`\n   |     return Ok(value);\n   |     ^^^^^^^^^^^^^^^^^ help: remove `return`: `Ok(value)`"
        );
    }

    // 4. VeryVerbose passes through rendered field
    #[test]
    fn very_verbose_uses_rendered() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::VeryVerbose,
        };
        let mut d = diag(
            "W1",
            DiagnosticLevel::Warning,
            Some("clippy::needless_return"),
            "unnecessary `return`",
        );
        d.rendered = Some("full rendered output here".to_string());
        assert_eq!(fmt.format_diagnostic(&d), "full rendered output here");
    }

    // 5. Success summary for clippy → "ok (clippy) 0 warnings 4.2s"
    #[test]
    fn success_summary_clippy() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Terse,
        };
        let s = summary("clippy", true, 0, 0, 4.2);
        assert_eq!(fmt.format_summary(&s), "ok (clippy) 0 warnings 4.2s");
    }

    // 6. Failure summary with counts → "2 warnings, 1 error 4.2s"
    #[test]
    fn failure_summary_with_counts() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Terse,
        };
        let s = summary("clippy", false, 2, 1, 4.2);
        assert_eq!(fmt.format_summary(&s), "2 warnings, 1 error 4.2s");
    }

    // 7. Test success summary → "ok (test) 47 passed, 0 failed 8.1s"
    #[test]
    fn test_success_summary() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Terse,
        };
        let s = RunSummary {
            command: "test".to_string(),
            success: true,
            errors: 0,
            warnings: 0,
            tests_passed: 47,
            tests_failed: 0,
            tests_ignored: 0,
            elapsed_secs: 8.1,
        };
        assert_eq!(fmt.format_summary(&s), "ok (test) 47 passed, 0 failed 8.1s");
    }

    // 8. Test failure terse → "F1 FAILED tests::parse_config"
    #[test]
    fn test_failure_terse() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Terse,
        };
        let r = TestResult {
            id: "F1".to_string(),
            name: "tests::parse_config".to_string(),
            passed: false,
            failure_message: None,
            file: None,
            line: None,
        };
        assert_eq!(fmt.format_test_failure(&r), "F1 FAILED tests::parse_config");
    }

    // 9. Test failure verbose → includes failure message and location
    #[test]
    fn test_failure_verbose() {
        let fmt = PlainFormatter {
            verbosity: Verbosity::Verbose,
        };
        let r = TestResult {
            id: "F1".to_string(),
            name: "tests::parse_config".to_string(),
            passed: false,
            failure_message: Some(
                "assertion `left == right` failed\n  left: None\n right: Some(\"default\")"
                    .to_string(),
            ),
            file: Some("src/config.rs".to_string()),
            line: Some(156),
        };
        assert_eq!(
            fmt.format_test_failure(&r),
            "F1 FAILED tests::parse_config\n   assertion `left == right` failed\n     left: None\n    right: Some(\"default\")\n   at src/config.rs:156"
        );
    }
}
