use std::process::Command;

fn cargo_terse() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_cargo-terse"));
    cmd.arg("terse");
    cmd
}

// All tests use current_dir("tests/fixtures/sample_project") so the cache is
// written to and read from that directory's `target/` folder, matching how
// cache::cache_dir() resolves the path at runtime.
const FIXTURE: &str = "tests/fixtures/sample_project";

#[test]
fn check_outputs_terse_warning() {
    let output = cargo_terse()
        .arg("check")
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should contain a terse warning line with hash-format ID (e.g. "W-98bf warning")
    assert!(
        stdout.lines().any(|l| {
            let mut parts = l.splitn(2, ' ');
            let id = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("");
            id.starts_with("W-") && rest.starts_with("warning")
        }),
        "expected W-<hash> warning in: {stdout}"
    );
    // Should contain "unused" somewhere
    assert!(stdout.contains("unused"), "expected 'unused' in: {stdout}");
    // Should NOT contain "Compiling" (that goes to stderr in the child)
    assert!(
        !stdout.contains("Compiling"),
        "should not contain Compiling progress"
    );
}

#[test]
fn check_outputs_summary() {
    let output = cargo_terse()
        .arg("check")
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("warning"),
        "expected warning summary in: {stdout}"
    );
    assert!(
        stdout.contains("ok (check)"),
        "expected ok summary in: {stdout}"
    );
}

#[test]
fn json_format_outputs_valid_jsonl() {
    let output = cargo_terse()
        .args(["--format", "json", "check"])
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    for line in stdout.lines() {
        let v: serde_json::Value = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("invalid JSON line: {e}\nline: {line}"));
        assert!(
            v.get("id").is_some() || v.get("summary").is_some(),
            "expected id or summary field in: {line}"
        );
    }
}

#[test]
fn test_command_shows_results() {
    let output = cargo_terse()
        .arg("test")
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Should show test count
    assert!(stdout.contains("passed"), "expected 'passed' in: {stdout}");
    // Should succeed
    assert!(output.status.success(), "expected success exit code");
}

#[test]
fn exit_code_mirrors_cargo() {
    // check should succeed (only warnings, no errors)
    let output = cargo_terse()
        .arg("check")
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "check with only warnings should exit 0"
    );
}

#[test]
fn detail_command_works_after_run() {
    // Run check with JSON format to get the actual hash-based ID from output.
    let run_output = cargo_terse()
        .args(["--format", "json", "check"])
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    let run_stdout = String::from_utf8(run_output.stdout).unwrap();

    // Extract the first diagnostic ID (W-xxxx) from JSON output.
    let id = run_stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|v| v.get("id").is_some() && v.get("summary").is_none())
        .and_then(|v| v["id"].as_str().map(str::to_owned))
        .expect("expected at least one diagnostic with an id");

    assert!(
        id.starts_with("W-"),
        "expected W-<hash> id, got: {id}"
    );

    // detail also runs with the same cwd so cache_dir() resolves to the same target/.
    let output = cargo_terse()
        .args(["detail", &id])
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.is_empty(), "detail {id} should produce output");
    assert!(
        stdout.contains("unused") || stdout.contains("extra"),
        "detail should contain the diagnostic: {stdout}"
    );
}

#[test]
fn verbose_check_includes_span() {
    let output = cargo_terse()
        .args(["-v", "check"])
        .current_dir(FIXTURE)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();
    // Verbose should include the span text (the actual code line)
    assert!(
        stdout.contains("|"),
        "verbose should include span with | prefix: {stdout}"
    );
}
