use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::cli::{OutputFormat, Verbosity};
use crate::diagnostic::{Diagnostic, DiagnosticLevel, RunResult, RunSummary};
use crate::format;
use crate::parser;

pub fn run_cargo(
    cargo_cmd: &str,
    cargo_args: &[OsString],
    format: &OutputFormat,
    verbosity: &Verbosity,
    no_cache: bool,
) -> i32 {
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
            return 1;
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

    let formatter = format::create_formatter(format, verbosity);
    let started = Instant::now();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut used_ids = std::collections::HashSet::new();
    // For test commands, non-JSON stdout lines contain test runner output.
    let mut test_stdout_lines: Vec<String> = Vec::new();
    let mut raw_bytes: usize = 0;
    let mut output_bytes: usize = 0;

    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            raw_bytes += line.len() + 1; // +1 for newline
            if let Some(diag) = parser::parse_cargo_json_line(&line, &mut used_ids) {
                let formatted = formatter.format_diagnostic(&diag);
                output_bytes += formatted.len() + 1;
                println!("{}", formatted);
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

    // Forward cargo's stderr when it failed and we captured no diagnostics.
    if exit_code != 0 && diagnostics.is_empty() && !stderr_output.is_empty() {
        eprint!("{}", stderr_output);
    }

    let errors = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Warning)
        .count();

    let (test_results, tests_passed, tests_failed, tests_ignored) = if is_test {
        // Test runner output goes to stdout (interleaved with JSON). Non-JSON lines were
        // collected in test_stdout_lines. Parse them the same way we parse stderr text.
        let test_output = test_stdout_lines.join("\n");
        let (results, summary) = parser::parse_test_stderr(&test_output);
        for result in &results {
            let formatted = formatter.format_test_failure(result);
            output_bytes += formatted.len() + 1;
            println!("{}", formatted);
        }
        (results, summary.passed, summary.failed, summary.ignored)
    } else {
        (vec![], 0, 0, 0)
    };

    let summary = RunSummary {
        command: cargo_cmd.to_owned(),
        success: exit_code == 0,
        errors,
        warnings,
        tests_passed,
        tests_failed,
        tests_ignored,
        elapsed_secs,
        raw_bytes,
        output_bytes: 0, // filled in after formatting
    };

    let summary_line = formatter.format_summary(&summary);
    output_bytes += summary_line.len() + 1;
    println!("{}", summary_line);
    // Update output_bytes in the summary for the cache.
    let summary = RunSummary {
        output_bytes,
        ..summary
    };

    let result = RunResult {
        diagnostics,
        test_results,
        summary,
    };

    if !no_cache {
        crate::cache::write_cache(&result);
    }

    exit_code
}
