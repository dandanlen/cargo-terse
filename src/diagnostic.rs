use serde::{Deserialize, Serialize};

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
    /// Full rendered output from rustc (for -vv and detail command)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub id: String,
    pub name: String,
    pub passed: bool,
    pub failure_message: Option<String>,
    pub file: Option<String>,
    pub line: Option<usize>,
}

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
    /// Raw bytes from cargo's stdout (JSON stream).
    #[serde(default)]
    pub raw_bytes: usize,
    /// Bytes of terse-formatted output.
    #[serde(default)]
    pub output_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub diagnostics: Vec<Diagnostic>,
    pub test_results: Vec<TestResult>,
    pub summary: RunSummary,
}
