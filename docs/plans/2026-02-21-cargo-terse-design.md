# cargo-terse Design

A cargo plugin that wraps cargo commands with concise, AI-optimized output.

## Problem

AI agents doing Rust development waste significant tokens on verbose cargo output: compilation progress, passing test names, multi-line diagnostic formatting, duplicate warnings, and help text. A concise wrapper that preserves only actionable information would reduce token cost and improve AI workflow efficiency.

## Command Interface

```
cargo terse <command> [cargo-flags...] [-- test-args...]
cargo terse detail <ID>
```

**Commands:** `build`, `check`, `test`, `clippy`, `fmt`

Bare `cargo terse` is equivalent to `cargo terse check`.

**Global flags:**
- `--format <plain|json|toon>` (default: `plain`)
- `-v` / `-vv` — verbosity level
- `--no-cache` — disable drill-down cache

All unrecognized flags pass through to cargo.

## Output Formats

### Plain Text (default)

**Default (terse):**
```
W1 warning[clippy::needless_return] src/main.rs:42:5 unnecessary `return`
W2 warning[unused_variables] src/lib.rs:17:9 unused variable `x`
E1 error[E0308] src/handler.rs:93:12 expected `u32`, found `&str`
```

**`-v` (with primary span):**
```
W1 warning[clippy::needless_return] src/main.rs:42:5 unnecessary `return`
   |     return Ok(value);
   |     ^^^^^^^^^^^^^^^^^ help: remove `return`: `Ok(value)`
```

**`-vv` (full rendered diagnostic):**
Passes through rustc's `rendered` field verbatim.

**Test output (default):**
```
test result: FAILED. 47 passed; 2 failed; 0 ignored

F1 FAILED tests::parse_config_missing_field
F2 FAILED tests::handler_timeout
```

**Test output (`-v`):**
```
F1 FAILED tests::parse_config_missing_field
   assertion `left == right` failed
     left: None
    right: Some("default")
   at src/config.rs:156
```

**Success:**
```
ok (build) 12.3s
ok (test) 47 passed, 0 failed 8.1s
ok (clippy) 0 warnings
ok (fmt) 0 files changed
```

**Summary line always present:**
```
2 warnings, 1 error
```

### JSON

JSONL, one object per diagnostic:
```json
{"id":"W1","level":"warning","code":"clippy::needless_return","file":"src/main.rs","line":42,"col":5,"message":"unnecessary `return`"}
{"id":"E1","level":"error","code":"E0308","file":"src/handler.rs","line":93,"col":12,"message":"expected `u32`, found `&str`"}
```

Test failures:
```json
{"id":"F1","level":"fail","test":"tests::parse_config_missing_field","message":"assertion `left == right` failed: left: None, right: Some(\"default\")","file":"src/config.rs","line":156}
```

Summary:
```json
{"summary":true,"command":"clippy","status":"fail","warnings":2,"errors":1,"elapsed_secs":4.2}
```

### TOON

Diagnostics as a TOON table:
```
diagnostics
  id level code file line col message
  W1 warning clippy::needless_return src/main.rs 42 5 unnecessary `return`
  W2 warning unused_variables src/lib.rs 17 9 unused variable `x`
  E1 error E0308 src/handler.rs 93 12 expected `u32`, found `&str`
```

## Drill-Down Cache

Each run writes `.terse-cache.json` to `$CARGO_TARGET_DIR/` (default `target/`):

```json
{
  "command": "clippy",
  "timestamp": "2026-02-21T14:30:00Z",
  "diagnostics": {
    "W1": { "rendered": "full rustc output...", "spans": [...], "children": [...] },
    "W2": {}
  }
}
```

`cargo terse detail W1` prints the full rendered diagnostic.
`cargo terse detail W1 --format json` returns full diagnostic JSON.

IDs are prefixed by type: `E` (error), `W` (warning), `F` (test failure). Numbered sequentially per run. Cache is overwritten each run (stateless).

## fmt Handling

`cargo fmt` doesn't support `--message-format=json`. Instead:

- Run `cargo fmt --check`, parse diff output to extract filenames
- Default: `ok (fmt) 0 files changed` or list files needing formatting
- `-v`: include first diff hunk per file
- `-vv`: full diff
- `cargo terse fmt --fix`: run `cargo fmt` and report what changed

## Architecture

```
cargo terse <cmd> [flags]
        |
        v
   +---------+
   |  CLI    |  Parse args (lexopt), separate terse flags from passthrough
   +----+----+
        |
        v
   +----------+
   |  Runner  |  Spawn `cargo <cmd> --message-format=json [flags]`
   +----+-----+
        | stdout (JSON lines)          | stderr (test output)
        v                              v
   +--------------+            +----------------+
   |  Diagnostic  |            |  Test Result   |
   |  Parser      |            |  Parser        |
   |  (JSON)      |            |  (stderr text) |
   +------+-------+            +-------+--------+
          |                            |
          v                            v
   +--------------------------------------+
   |  Formatter                           |
   |  (plain / json / toon)              |
   |  x (terse / -v / -vv)              |
   +------+-------------------------------+
          |                    |
          v                    v
     stdout (streaming)   .terse-cache.json
```

**Components:**
- **CLI**: `lexopt`-based arg parsing
- **Runner**: Spawns cargo as child process, pipes stdout/stderr, forwards exit code
- **Diagnostic Parser**: Reads JSON lines, filters to `compiler-message`, extracts fields, assigns IDs
- **Test Result Parser**: Regex on libtest stderr output (stable Rust). Future: swap to libtest JSON when stabilized
- **Formatter**: Trait with plain/json/toon implementations. Verbosity controls detail level
- **Cache Writer**: Writes full diagnostic data alongside streaming output

## Dependencies

- `lexopt` — CLI arg parsing (zero deps)
- `serde`, `serde_json` — JSON parsing
- `toon` — TOON output (if Rust crate exists, otherwise hand-write table format)

## Exit Codes

Mirrors cargo: `0` success, `1` failure, `101` cargo crash.

## Roadmap

- **Post-MVP**: Native libtest JSON parsing when `--format json` stabilizes (currently nightly-only via `-Z unstable-options`)
- **Post-MVP**: `cargo terse` as a meta-command running check+clippy+fmt in one invocation
