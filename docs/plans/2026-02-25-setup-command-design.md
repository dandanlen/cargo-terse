# cargo terse setup — Design

## Overview

`cargo terse setup` auto-configures AI agent instruction files with cargo-terse usage instructions. Idempotent, multi-agent, project or global scope.

## CLI

```
cargo terse setup              # auto-detect project agent configs
cargo terse setup --global     # write to ~/.claude/CLAUDE.md
cargo terse setup --agent cursor  # target specific agent
```

## Agent config files (project-level)

| Agent     | File                              |
|-----------|-----------------------------------|
| claude    | CLAUDE.md                         |
| agents    | AGENTS.md                         |
| cursor    | .cursorrules                      |
| copilot   | .github/copilot-instructions.md   |

Auto-detect: scan current directory for which exist. If none exist, create CLAUDE.md. `--agent <name>` targets one specifically (creates if needed). `--global` writes to `~/.claude/CLAUDE.md` only.

## Snippet

```markdown
## cargo-terse

Use `cargo terse` instead of `cargo` for build, check, test, and clippy commands.
Examples:
- `cargo terse check` instead of `cargo check`
- `cargo terse test` instead of `cargo test`
- `cargo terse clippy` instead of `cargo clippy`

When a diagnostic needs more context, use `cargo terse detail <ID>` to expand it.
```

## Idempotency

Substring search for `"cargo terse"` in existing file content. If found, skip with message.

## Output

```
cargo-terse: appended instructions to CLAUDE.md
cargo-terse: .cursorrules already configured, skipping
```

## Implementation

- New `Command::Setup { global: bool, agent: Option<String> }` variant in cli.rs
- New `setup.rs` module with all logic
- Update help text and README
