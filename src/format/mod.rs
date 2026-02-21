mod json;
mod plain;
mod toon;

pub use json::JsonFormatter;
pub use plain::PlainFormatter;
pub use toon::ToonFormatter;

use crate::cli::{OutputFormat, Verbosity};
use crate::diagnostic::{Diagnostic, RunSummary, TestResult};

pub trait Formatter {
    fn format_diagnostic(&self, diag: &Diagnostic) -> String;
    fn format_test_failure(&self, result: &TestResult) -> String;
    fn format_summary(&self, summary: &RunSummary) -> String;
}

pub fn create_formatter(format: &OutputFormat, verbosity: &Verbosity) -> Box<dyn Formatter> {
    match format {
        OutputFormat::Plain => Box::new(PlainFormatter { verbosity: verbosity.clone() }),
        OutputFormat::Json => Box::new(JsonFormatter),
        OutputFormat::Toon => Box::new(ToonFormatter),
    }
}
