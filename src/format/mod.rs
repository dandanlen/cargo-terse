mod plain;
pub use plain::PlainFormatter;

use crate::diagnostic::{Diagnostic, TestResult, RunSummary};

pub trait Formatter {
    fn format_diagnostic(&self, diag: &Diagnostic) -> String;
    fn format_test_failure(&self, result: &TestResult) -> String;
    fn format_summary(&self, summary: &RunSummary) -> String;
}
