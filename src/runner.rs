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
    let is_fmt = cargo_cmd == "fmt";
    let is_test = cargo_cmd == "test";

    let mut cmd = Command::new(&cargo_bin);
    cmd.arg(cargo_cmd);
    if !is_fmt {
        cmd.arg("--message-format=json");
    }
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
    let mut next_warning_id = 0usize;
    let mut next_error_id = 0usize;

    if !is_fmt {
        if let Some(stdout) = child.stdout.take() {
            for line in BufReader::new(stdout).lines() {
                let line = match line {
                    Ok(l) => l,
                    Err(_) => break,
                };
                if let Some(diag) =
                    parser::parse_cargo_json_line(&line, &mut next_warning_id, &mut next_error_id)
                {
                    print!("{}", formatter.format_diagnostic(&diag));
                    diagnostics.push(diag);
                }
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

    let errors = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|d| d.level == DiagnosticLevel::Warning)
        .count();

    let (test_results, tests_passed, tests_failed, tests_ignored) = if is_test {
        let (results, summary) = parser::parse_test_stderr(&stderr_output);
        for result in &results {
            print!("{}", formatter.format_test_failure(result));
        }
        let (passed, failed, ignored) = (summary.passed, summary.failed, summary.ignored);
        (results, passed, failed, ignored)
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
    };

    print!("{}", formatter.format_summary(&summary));

    let result = RunResult {
        diagnostics,
        test_results,
        summary,
    };

    if !no_cache {
        write_cache(&result);
    }

    exit_code
}

fn write_cache(_result: &RunResult) {
    // TODO: implement in Task 10
}
