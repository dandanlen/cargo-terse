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
    let fmt_fix = is_fmt && cargo_args.iter().any(|a| a == "--fix");
    let fmt_has_check = is_fmt && cargo_args.iter().any(|a| a == "--check");
    let fmt_check = is_fmt && !fmt_fix;

    let mut cmd = Command::new(&cargo_bin);
    cmd.arg(cargo_cmd);
    if !is_fmt {
        cmd.arg("--message-format=json");
    }
    if fmt_check && !fmt_has_check {
        cmd.arg("--check");
    }
    if is_fmt {
        cmd.args(cargo_args.iter().filter(|a| *a != "--fix"));
    } else {
        cmd.args(cargo_args);
    }
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

    let elapsed_secs = started.elapsed().as_secs_f64();

    // Handle fmt output separately.
    if is_fmt {
        if exit_code != 0 && !stderr_output.is_empty() {
            eprint!("{}", stderr_output);
        }
        return run_fmt_output(
            fmt_fix,
            &fmt_stdout,
            exit_code,
            elapsed_secs,
            verbosity,
            format,
            formatter.as_ref(),
        );
    }

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
    format: &OutputFormat,
    formatter: &dyn format::Formatter,
) -> i32 {
    let make_summary = |success: bool, warnings: usize| RunSummary {
        command: "fmt".to_string(),
        success,
        errors: 0,
        warnings,
        tests_passed: 0,
        tests_failed: 0,
        tests_ignored: 0,
        elapsed_secs,
    };

    if is_fix {
        println!("{}", formatter.format_summary(&make_summary(true, 0)));
        return exit_code;
    }

    // --check mode: parse the diff.
    let fmt_result = fmt_mod::parse_fmt_output(stdout);

    if fmt_result.files.is_empty() {
        println!("{}", formatter.format_summary(&make_summary(true, 0)));
        return exit_code;
    }

    // Per-file output.
    match format {
        OutputFormat::Plain => {
            for (i, file) in fmt_result.files.iter().enumerate() {
                println!("F{} unformatted {file}", i + 1);

                let (show_diff, compact) = match verbosity {
                    Verbosity::Verbose => (true, true),
                    Verbosity::VeryVerbose => (true, false),
                    Verbosity::Terse => (false, false),
                };
                if show_diff {
                    let diff = fmt_mod::format_file_diff(&fmt_result.full_diff, file, compact);
                    for line in diff.lines() {
                        println!("   {line}");
                    }
                }
            }
        }
        _ => {
            for (i, file) in fmt_result.files.iter().enumerate() {
                let diag = Diagnostic {
                    id: format!("F{}", i + 1),
                    level: DiagnosticLevel::Warning,
                    code: Some("unformatted".to_string()),
                    message: "needs formatting".to_string(),
                    file: Some(file.clone()),
                    line: None,
                    col: None,
                    span_text: None,
                    span_label: None,
                    rendered: None,
                    raw_json: None,
                };
                println!("{}", formatter.format_diagnostic(&diag));
            }
        }
    }

    let n = fmt_result.files.len();
    println!("{}", formatter.format_summary(&make_summary(false, n)));

    exit_code
}
