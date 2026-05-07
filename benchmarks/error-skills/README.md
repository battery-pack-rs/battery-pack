# Error Skills Benchmark

Manual testing harness for verifying skills work correctly via Symposium integration.

## Quick start

```bash
# Full run with default target (/tmp/error-skills-bench-target)
# Clones mini-redis automatically if target doesn't exist
./run.sh

# With explicit target
./run.sh --target /path/to/mini-redis

# Regenerate from scratch
./run.sh --clean
```

## What it does

1. `setup.sh`: clones mini-redis if needed, runs `cargo bp add error`, then `cargo agents sync`
2. `run.sh`: calls setup if skills aren't installed, then runs `claude -p` with streaming output

## Output files

Each run produces (in `/tmp/`):
- `skill-benchmark-*.md` (agent text output)
- `skill-benchmark-*.raw` (full JSON stream)
- `skill-benchmark-*.tools` (tool usage summary)
- `skill-benchmark-*.skills` (skills invoked)
- `skill-benchmark-*.commands` (bash commands run)

## Evaluating results

```
Evaluate /tmp/skill-benchmark-<timestamp>.md against benchmarks/error-skills/EXPECTED.md
```

## Prerequisites

- `cargo-bp` installed (`cargo install cargo-bp`)
- `symposium` installed (`cargo install symposium`)
- `claude` CLI authenticated
- Battery-pack plugin source registered in `~/.symposium/config.toml`:
  ```toml
  plugin-source = [
      { name = "battery-pack", path = "/path/to/battery-pack/battery-packs/error-battery-pack" },
  ]
  ```

## Agent support

Currently Claude Code only (`claude -p`). Future harnesses:

- Kiro (`kiro chat`)
- Codex (`codex --quiet`)
- Interactive session evaluation (manual paste + human scoring)
