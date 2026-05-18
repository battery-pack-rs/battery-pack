# CI Skills Benchmark

Manual testing harness for verifying CI skills work correctly via Symposium integration.

## Quick start

```bash
# Full run with default target (/tmp/ci-skills-bench-target)
# Creates a fresh Rust project automatically if target doesn't exist
./run.sh

# With explicit target
./run.sh --target /path/to/my-project

# Regenerate from scratch
./run.sh --clean
```

## What it does

1. `setup.sh`: creates a fresh Rust library+binary project if needed, runs `cargo bp add ci`, then `cargo agents sync`
2. `run.sh`: calls setup if skills aren't installed, then runs `claude -p` with streaming output

## Output files

Each run produces (in `/tmp/`):
- `ci-skill-benchmark-*.md` (agent text output)
- `ci-skill-benchmark-*.raw` (full JSON stream)
- `ci-skill-benchmark-*.tools` (tool usage summary)
- `ci-skill-benchmark-*.skills` (skills invoked)
- `ci-skill-benchmark-*.thinking` (internal reasoning)

## Evaluating results

```
Evaluate /tmp/ci-skill-benchmark-<timestamp>.md against benchmarks/ci-skills/EXPECTED.md
```

## Prerequisites

- `cargo-bp` installed (`cargo install cargo-bp`)
- `symposium` installed (`cargo install symposium`)
- `claude` CLI authenticated
- Battery-pack plugin source registered in `~/.symposium/config.toml`:
  ```toml
  plugin-source = [
      { name = "battery-pack", path = "/path/to/battery-pack/battery-packs/ci-battery-pack" },
  ]
  ```

## Agent support

Currently Claude Code only (`claude -p`). Future harnesses:

- Kiro (`kiro chat`)
- Codex (`codex --quiet`)
- Interactive session evaluation (manual paste + human scoring)
