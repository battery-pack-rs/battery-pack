# Network Service Skills Benchmark

Manual testing harness that exercises the network-service battery pack's skills end to end. A single run has two phases: first the agent bootstraps a freshly generated service (build, run, curl every endpoint, enable dial9 and poke around the trace with its agent tooling, run the benchmarks), then it layers on production features (per-client rate limiting, a read cache, load shedding) using the skills as breadcrumbs. Phase 1 is a "does it bootstrap and is it observable" check, not a performance benchmark; the prompt tells the agent not to draw performance conclusions.

## Quick start

```bash
./run.sh            # default target /tmp/network-service-skills-target
./run.sh --clean    # regenerate from scratch
```

## What it does

1. `setup.sh`: `cargo bp new network-service` with dial9, jemalloc, rate-limit, and benchmarks enabled, then `cargo agents sync` (which installs the pack's skills and, since the project depends on dial9, dial9's own agent skills).
2. `run.sh`: runs `claude -p` with the two-phase prompt, streams it live, and assembles a single self-contained report at `$LOG.md` (`/tmp/network-service-skills-<timestamp>.md`) ready to paste into a gist: run metadata (prompt, model, agent, duration, turn/cost), the invoked skills and bash commands, the full transcript, and the raw JSON event stream collapsed at the bottom. The server is backgrounded with redirected output and killed so the run does not hang; the agent uses `dial9 agents` rather than the long-running `dial9 serve` web UI.

## Evaluating results

```
Evaluate /tmp/network-service-skills-<timestamp>.md against benchmarks/network-service-skills/EXPECTED.md
```

## Prerequisites

- `cargo-bp` and `symposium` installed (`cargo install cargo-bp symposium`)
- `claude` CLI authenticated
- `dial9` CLI for the trace poking (`cargo install --locked dial9`)
- Optional: `oha` for load generation (the run falls back to a curl loop without it); running with `--downstream-url` makes the trace more representative by exercising the forwarding path
- The generated `.cargo/config.toml` already sets the `tokio_unstable` and frame-pointer flags dial9 needs; dial9 CPU profiling may need relaxed `perf_event_paranoid` on the host
- Battery-pack plugin source registered in `~/.symposium/config.toml`:
  ```toml
  plugin-source = [
      { name = "network-service", path = "/path/to/battery-pack/battery-packs/network-service-battery-pack" },
  ]
  ```

## Agent support

Currently Claude Code only (`claude -p`).
