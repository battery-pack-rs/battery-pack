# Network Service Features Benchmark

Manual testing harness that checks whether an agent can extend the generated service with the production features the skills describe as *breadcrumbs*. The skills deliberately do not give full recipes for these tack-on features (they name the crate, the one decisive pitfall, and point upstream), so this benchmark measures whether an agent can follow those pointers and the crates' own docs to a correct implementation.

The three asks map to the `service-architecture` skill: per-client rate limiting, a read-through cache (moka), and load shedding (tower).

## Quick start

```bash
./run.sh            # default target /tmp/network-service-features-target
./run.sh --clean    # regenerate from scratch
```

## What it does

1. `setup.sh`: `cargo bp new network-service` (rate-limit on, so there is a global limiter to upgrade), then `cargo agents sync`.
2. `run.sh`: runs `claude -p` with the feature-extension prompt and captures streaming output, tool usage, and invoked skills.

## Evaluating results

```
Evaluate /tmp/network-service-features-<timestamp>.md against benchmarks/network-service-features/EXPECTED.md
```

## Prerequisites

Same as the bootstrap benchmark: `cargo-bp`, `symposium`, an authenticated `claude` CLI, and the network-service plugin source registered in `~/.symposium/config.toml`.
