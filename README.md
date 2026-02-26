# cargo-terse

Concise cargo output for AI-assisted Rust development.

`cargo-terse` wraps `cargo build`, `check`, `test`, and `clippy` with condensed output optimized for AI agent consumption. Fewer tokens, same information.

## Install

### From crates.io
```bash
cargo install cargo-terse
```

### Pre-built binaries (via cargo-binstall)
```bash
cargo binstall cargo-terse
```

### From source
```bash
git clone https://github.com/dandanlen/cargo-terse.git
cd cargo-terse
cargo install --path .
```

## Usage

```bash
cargo terse check           # concise check output
cargo terse test             # test results without noise
cargo terse clippy           # terse clippy warnings
cargo terse                  # defaults to check
```

### Setup

```bash
cargo terse setup                  # auto-detect and configure agent instruction files
cargo terse setup --global         # configure ~/.claude/CLAUDE.md
cargo terse setup --agent cursor   # configure .cursorrules specifically
```

Auto-detects existing agent config files (CLAUDE.md, AGENTS.md, .cursorrules, .github/copilot-instructions.md) and appends usage instructions. Idempotent — won't duplicate if already configured.

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

## Configuration for AI agents

Add to your project's `CLAUDE.md` (or equivalent AI agent instructions):

```
Use `cargo terse` instead of `cargo` for build, check, test, and clippy commands.
Examples:
- `cargo terse check` instead of `cargo check`
- `cargo terse test` instead of `cargo test`
- `cargo terse clippy` instead of `cargo clippy`

When a diagnostic needs more context, use `cargo terse detail <ID>` to expand it.
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

## Why not --message-format=short?

`cargo check --message-format=short` has existed since 2018 and reduces diagnostic verbosity. `cargo-terse` builds beyond it:

- **Stable drill-down IDs** (W1, E1, F1) let you revisit any diagnostic with `cargo terse detail <ID>` without re-running the build
- **Unified output** across `check`, `test`, and `clippy` in a single consistent format
- **Multiple output formats** (plain, JSON, TOON) suited to different consumers
- Pairs well with [RTK](https://github.com/dandanlen/rtk) for additional token savings on broader cargo output

## How it works

`cargo-terse` spawns cargo with `--message-format=json` (rather than the simpler `--message-format=short`), parses the JSON stream in real-time, and re-renders diagnostics in a condensed format. The JSON stream provides enough detail to assign stable IDs and support drill-down. Test results are parsed from stdout text output. A cache file (`target/.terse-cache.json`) enables the `detail` drill-down command.

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
