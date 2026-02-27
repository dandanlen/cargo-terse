use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::cli::{OutputFormat, Verbosity};
use crate::diagnostic::{Diagnostic, DiagnosticLevel, RunResult, RunSummary, TestResult};
use crate::format;
use crate::parser;

pub struct CargoOutput {
    pub diagnostics: Vec<Diagnostic>,
    pub test_results: Vec<TestResult>,
    pub cargo_cmd: String,
    pub exit_code: i32,
    pub stderr_output: String,
    pub elapsed_secs: f64,
    pub raw_bytes: usize,
    /// Count of ignored tests, sourced from the test runner summary line.
    pub tests_ignored: usize,
}

/// Run cargo and collect results without printing.
pub fn execute_cargo(cargo_cmd: &str, cargo_args: &[OsString]) -> CargoOutput {
    let cargo_bin = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let is_test = cargo_cmd == "test";

    let mut cmd = Command::new(&cargo_bin);
    cmd.arg(cargo_cmd);
    cmd.arg("--message-format=json");
    cmd.args(cargo_args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cargo-terse: failed to spawn cargo: {e}");
            return CargoOutput {
                diagnostics: vec![],
                test_results: vec![],
                cargo_cmd: cargo_cmd.to_owned(),
                exit_code: 1,
                stderr_output: String::new(),
                elapsed_secs: 0.0,
                raw_bytes: 0,
                tests_ignored: 0,
            };
        }
    };

    if std::io::IsTerminal::is_terminal(&std::io::stderr()) {
        eprint!("\x1b[2Kcargo {cargo_cmd}...\r");
    }

    // Collect stderr in a background thread to avoid deadlock when the pipe buffer fills.
    let stderr_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || {
            let mut buf = String::new();
            BufReader::new(stderr).read_to_string(&mut buf).ok();
            buf
        })
    });

    let started = Instant::now();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut used_ids = std::collections::HashSet::new();
    // For test commands, non-JSON stdout lines contain test runner output.
    let mut test_stdout_lines: Vec<String> = Vec::new();
    let mut raw_bytes: usize = 0;

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            raw_bytes += line.len() + 1; // +1 for newline
            if let Some(diag) = parser::parse_cargo_json_line(&line, &mut used_ids) {
                diagnostics.push(diag);
            } else if is_test {
                test_stdout_lines.push(line);
            }
        }
    }

    let exit_code = match child.wait() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("cargo-terse: failed to wait on cargo: {e}");
            1
        }
    };

    let stderr_output = stderr_handle.map(|h| h.join().unwrap()).unwrap_or_default();
    let elapsed_secs = started.elapsed().as_secs_f64();

    let (test_results, tests_ignored) = if is_test {
        // Test runner output goes to stdout (interleaved with JSON). Non-JSON lines were
        // collected in test_stdout_lines. Parse them the same way we parse stderr text.
        let test_output = test_stdout_lines.join("\n");
        let (results, summary) = parser::parse_test_stderr(&test_output);
        (results, summary.ignored)
    } else {
        (vec![], 0)
    };

    CargoOutput {
        diagnostics,
        test_results,
        cargo_cmd: cargo_cmd.to_owned(),
        exit_code,
        stderr_output,
        elapsed_secs,
        raw_bytes,
        tests_ignored,
    }
}

/// Print collected results using the given formatter, returning the full RunResult for caching.
pub fn display_results(output: &CargoOutput, formatter: &dyn format::Formatter) -> RunResult {
    // Forward cargo's stderr when it failed and we captured no diagnostics.
    if output.exit_code != 0 && output.diagnostics.is_empty() && !output.stderr_output.is_empty() {
        eprint!("{}", output.stderr_output);
    }

    let mut output_bytes: usize = 0;

    for diag in &output.diagnostics {
        let formatted = formatter.format_diagnostic(diag);
        output_bytes += formatted.len() + 1;
        println!("{}", formatted);
    }

    for result in &output.test_results {
        let formatted = formatter.format_test_failure(result);
        output_bytes += formatted.len() + 1;
        println!("{}", formatted);
    }

    let errors = output.diagnostics.iter().filter(|d| d.level == DiagnosticLevel::Error).count();
    let warnings = output.diagnostics.iter().filter(|d| d.level == DiagnosticLevel::Warning).count();
    let tests_passed = output.test_results.iter().filter(|r| r.passed).count();
    let tests_failed = output.test_results.iter().filter(|r| !r.passed).count();

    let summary = RunSummary {
        command: output.cargo_cmd.clone(),
        success: output.exit_code == 0,
        errors,
        warnings,
        tests_passed,
        tests_failed,
        tests_ignored: output.tests_ignored,
        elapsed_secs: output.elapsed_secs,
        raw_bytes: output.raw_bytes,
        output_bytes: 0, // filled in after formatting the summary line
    };

    let summary_line = formatter.format_summary(&summary);
    output_bytes += summary_line.len() + 1;
    println!("{}", summary_line);

    RunResult {
        diagnostics: output.diagnostics.clone(),
        test_results: output.test_results.clone(),
        summary: RunSummary { output_bytes, ..summary },
    }
}

pub fn run_cargo(
    cargo_cmd: &str,
    cargo_args: &[OsString],
    format: &OutputFormat,
    verbosity: &Verbosity,
    no_cache: bool,
) -> i32 {
    let output = execute_cargo(cargo_cmd, cargo_args);
    let exit_code = output.exit_code;
    let formatter = format::create_formatter(format, verbosity);
    let result = display_results(&output, &*formatter);

    if !no_cache {
        crate::cache::write_cache(&result);
    }

    exit_code
}
