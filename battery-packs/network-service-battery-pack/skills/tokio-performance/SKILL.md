---
name: tokio-performance
description: Runtime tuning and scheduler diagnostics for the network-service scaffold, pointing at dial9 and the BuilderHub Tokio guide
---

# Tokio Performance

What this scaffold wired for Tokio runtime visibility, and where to go to actually diagnose a slow runtime. This skill is intentionally short: the deep material already exists upstream (see References), so this only covers the scaffold-specific pieces.

## What the scaffold wired

- **`.cargo/config.toml`** always sets `--cfg tokio_unstable`, which exposes the unstable runtime-metrics APIs the tokio-metrics bridge samples; removing it breaks runtime metric collection. It is an API-stability flag (these APIs may shift between Tokio releases), not a production-safety concern. When the `dial9` feature is on, the config also sets `-C force-frame-pointers=yes` so dial9 can symbolize stacks cheaply; that flag is generated only with dial9.
- **Runtime metrics** are surfaced through metrique's tokio bridge (`subscribe_tokio_runtime_metrics`), so worker busy ratio, queue depth, and similar land in your normal metric stream for dashboards and alarms.
- **`dial9::spawn`** is used instead of `tokio::spawn` (when the `dial9` feature is on) so background tasks show up in the trace rather than vanishing.

## When runtime tuning matters

Reach for this when request latency is high but handler work is not: the time is being spent *waiting to be polled*, not *running*. That shows up as scheduling delay or rising queue depth in the runtime metrics while CPU is not saturated. Common causes are blocking calls on the async runtime (move them to `spawn_blocking`), too few worker threads, or a few tasks monopolizing workers. The runtime metrics tell you *that* it is happening; dial9 tells you *which task*.

## Invariants

- Keep `tokio_unstable` while tokio-metrics or dial9 are in use; keep `force-frame-pointers` while dial9 is in use (it is generated only with dial9).
- Use `dial9::spawn` (not `tokio::spawn`) for background tasks when the `dial9` feature is on, or they are invisible to the profiler.

## References

- BuilderHub, Diagnosing performance issues in Tokio-based network services: https://docs.hub.amazon.dev/docs/languages/rust/howto-understanding-tokio-performance/
- tokio-metrics: https://docs.rs/tokio-metrics
- dial9 trace reading and the viewer: see the dial9 references in the `telemetry` skill (dial9 ships its own analysis skills).
