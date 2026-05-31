# Network Service Bootstrap Benchmark

Manual testing harness that checks whether an agent can take a freshly generated network-service project from `cargo bp new` all the way to a running, observable service. This is a "does it bootstrap" check, not a performance benchmark: the prompt explicitly tells the agent not to draw performance conclusions.

## Quick start

```bash
# Full run with default target (/tmp/network-service-bootstrap-target)
# Generates the service automatically if the target does not exist
./run.sh

# Regenerate from scratch
./run.sh --clean
```

## What it does

1. `setup.sh`: runs `cargo bp new network-service` (with dial9, jemalloc, rate-limit, and benchmarks enabled) to generate the service into the target, then `cargo agents sync` to install the pack's skills.
2. `run.sh`: calls setup if skills aren't installed, then runs `claude -p` with a prompt that asks the agent to build, run, curl, enable dial9, and run the benchmarks, capturing streaming output.

## Output files

Each run produces (in `/tmp/`): `network-service-bootstrap-*.md` (agent text), `.raw` (JSON stream), `.tools`, `.skills`, `.commands`.

## Evaluating results

```
Evaluate /tmp/network-service-bootstrap-<timestamp>.md against benchmarks/network-service-bootstrap/EXPECTED.md
```

## Prerequisites

- `cargo-bp` installed (`cargo install cargo-bp`)
- `symposium` installed (`cargo install symposium`)
- `claude` CLI authenticated
- A nightly-ish toolchain is not required, but dial9 needs `tokio_unstable` and frame pointers, which the generated `.cargo/config.toml` already sets
- For dial9 CPU profiling specifically, the host may need `kernel.perf_event_paranoid` lowered; the basic runtime trace does not
- Battery-pack plugin source registered in `~/.symposium/config.toml`:
  ```toml
  plugin-source = [
      { name = "network-service", path = "/path/to/battery-pack/battery-packs/network-service-battery-pack" },
  ]
  ```

## Agent support

Currently Claude Code only (`claude -p`). The harness mirrors the error-skills and ci-skills benchmarks; the same future agents (Kiro, Codex) apply.
