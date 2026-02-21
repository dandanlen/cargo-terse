use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::cli::{OutputFormat, Verbosity};
use crate::diagnostic::{Diagnostic, DiagnosticLevel, RunResult, RunSummary};
use crate::fmt as fmt_mod;
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

    // For fmt, determine whether to run --check or --fix mode.
    // --fix: user explicitly passed --fix; we run plain `cargo fmt` (no --check).
    // --check: user explicitly passed --check; honour it, don't add another one.
    // default: add --check so we can parse the diff.
    let fmt_fix = is_fmt
        && cargo_args
            .iter()
            .any(|a| a == "--fix");
    let fmt_has_check = is_fmt
        && cargo_args
            .iter()
            .any(|a| a == "--check");
    let fmt_check = is_fmt && !fmt_fix;

    let mut cmd = Command::new(&cargo_bin);
    cmd.arg(cargo_cmd);
    if !is_fmt {
        cmd.arg("--message-format=json");
    }
    if fmt_check && !fmt_has_check {
        cmd.arg("--check");
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
    // For test commands, non-JSON stdout lines contain test runner output.
    let mut test_stdout_lines: Vec<String> = Vec::new();
    // For fmt --check, collect stdout (the diff output).
    let mut fmt_stdout = String::new();

    if is_fmt {
        if let Some(stdout) = child.stdout.take() {
            BufReader::new(stdout).read_to_string(&mut fmt_stdout).ok();
        }
    } else if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            if let Some(diag) =
                parser::parse_cargo_json_line(&line, &mut next_warning_id, &mut next_error_id)
            {
                println!("{}", formatter.format_diagnostic(&diag));
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
    let _ = stderr_output; // available for future use

    let elapsed_secs = started.elapsed().as_secs_f64();

    // Handle fmt output separately.
    if is_fmt {
        return run_fmt_output(
            fmt_fix,
            &fmt_stdout,
            exit_code,
            elapsed_secs,
            verbosity,
            formatter.as_ref(),
        );
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
            println!("{}", formatter.format_test_failure(result));
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
    };

    println!("{}", formatter.format_summary(&summary));

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

/// Print fmt results and return the exit code.
fn run_fmt_output(
    is_fix: bool,
    stdout: &str,
    exit_code: i32,
    elapsed_secs: f64,
    verbosity: &Verbosity,
    formatter: &dyn format::Formatter,
) -> i32 {
    let elapsed = format!("{:.1}s", elapsed_secs);

    if is_fix {
        // `cargo fmt` (fix mode): just report ok.
        println!("ok (fmt) {elapsed}");
        return exit_code;
    }

    // --check mode: parse the diff.
    let fmt_result = fmt_mod::parse_fmt_output(stdout);

    if fmt_result.files.is_empty() {
        println!("ok (fmt) {elapsed}");
        return exit_code;
    }

    // Print a line per file, then a summary.
    for (i, file) in fmt_result.files.iter().enumerate() {
        let id = i + 1;
        println!("F{id} unformatted {file}");

        match verbosity {
            Verbosity::Verbose => {
                let diff = fmt_mod::format_file_diff(&fmt_result.full_diff, file, true);
                if !diff.is_empty() {
                    for line in diff.lines() {
                        println!("   {line}");
                    }
                }
            }
            Verbosity::VeryVerbose => {
                let diff = fmt_mod::format_file_diff(&fmt_result.full_diff, file, false);
                if !diff.is_empty() {
                    for line in diff.lines() {
                        println!("   {line}");
                    }
                }
            }
            Verbosity::Terse => {}
        }
    }

    let n = fmt_result.files.len();
    println!(
        "{n} {} formatting {elapsed}",
        if n == 1 { "file needs" } else { "files need" }
    );

    // Suppress unused warning on formatter for fmt path (the trait object is
    // created before we know it's fmt; in future other formatters could use it).
    let _ = formatter;

    exit_code
}

