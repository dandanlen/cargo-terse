# cargo-terse

Concise cargo output for AI-assisted Rust development.

`cargo-terse` wraps `cargo build`, `check`, `test`, `clippy`, and `fmt` with condensed output optimized for AI agent consumption. Fewer tokens, same information.

## Install

```bash
cargo install cargo-terse
```

## Usage

```bash
cargo terse check           # concise check output
cargo terse test             # test results without noise
cargo terse clippy           # terse clippy warnings
cargo terse fmt              # check formatting
cargo terse                  # defaults to check
```

### Output formats

```bash
cargo terse --format plain check   # default: one-line-per-diagnostic
cargo terse --format json check    # JSONL output
cargo terse --format toon check    # TOON (Token-Oriented Object Notation)
```

### Verbosity

```bash
cargo terse check          # terse (default): one-liner per diagnostic
cargo terse -v check       # verbose: includes code span
cargo terse -vv check      # very verbose: full rustc rendered output
```

### Drill-down

Each diagnostic gets an ID. Use `detail` to expand it:

```bash
cargo terse check          # shows: W1 warning[unused_variables] src/lib.rs:4:9 ...
cargo terse detail W1      # shows full rendered diagnostic for W1
```

## Example output

**Default (terse):**
```
W1 warning[unused_variables] src/lib.rs:4:9 unused variable `extra`
ok (check) 1 warning 0.1s
```

**JSON:**
```json
{"id":"W1","level":"warning","code":"unused_variables","file":"src/lib.rs","line":4,"col":9,"message":"unused variable `extra`"}
{"summary":true,"command":"check","status":"ok","warnings":1,"errors":0,"elapsed_secs":0.1}
```

**Test output:**
```
ok (test) 36 passed, 0 failed 0.5s
```

**With failures:**
```
F1 FAILED tests::parse_config_missing_field
F2 FAILED tests::handler_timeout
test result: FAILED. 2 passed; 2 failed; 0 ignored 1.2s
```

## How it works

`cargo-terse` spawns cargo with `--message-format=json`, parses the JSON stream in real-time, and re-renders diagnostics in a condensed format. Test results are parsed from stdout text output. A cache file (`target/.terse-cache.json`) enables the `detail` drill-down command.

## Flags

| Flag | Description |
|------|-------------|
| `--format <plain\|json\|toon>` | Output format (default: plain) |
| `-v` | Verbose: include code span |
| `-vv` | Very verbose: full rustc output |
| `--no-cache` | Disable drill-down cache |

All other flags pass through to cargo.

## License

MIT
