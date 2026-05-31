---
name: telemetry
description: Observability wiring for the network-service scaffold: metrique wide-event metrics, tracing logs, and optional dial9 runtime profiling
---

# Telemetry

How this service emits metrics and logs, and how to read its runtime behavior. The wiring lives in `src/telemetry.rs` (init), `src/metrics.rs` (the metric records), and `src/middleware.rs` (the per-request recorder).

## Why one wide event per request

Every request emits a single `RequestMetrics` record carrying all of its dimensions (operation, status, latency, request id, payload bytes, downstream outcome). One row per request means you answer questions ("p99 latency for SetItem when the downstream timed out") with a filter on one stream, not a join across separate counter and timer streams. Add a new dimension by adding a field to `RequestMetrics` or to the handler-owned `HandlerMetrics` slot, never by emitting a second record.

## metrique and dial9 do different jobs

This scaffold splits observability across two tools on purpose:

- **metrique** owns the business and RED metrics (rate, errors, duration) you alarm and dashboard on. It also bridges Tokio runtime metrics (queue depth, busy ratio) via `subscribe_tokio_runtime_metrics`.
- **dial9** (optional, off by default) is an always-on Tokio profiler for root-cause work: poll timing, park/unpark, wake, per-task CPU. You reach for it when the metrics say "slow" but not "why".

Keep that boundary: do not reimplement runtime sampling in metrique, and do not push request KPIs into dial9 traces.

## Where output goes

`--telemetry-dir` (env `TELEMETRY_DIR`) selects the destination at runtime. Unset (the default) sends logs to stderr and metrics to stdout on separate streams, which is what you want for local runs and container log capture. Set it to a directory and both roll into files there (`application.log` hourly, `metrics.log` minutely). Logs use a non-blocking writer so a slow or full disk never blocks request handling; the `TelemetryGuard` returned by `init_telemetry` must stay alive for the life of the process or buffered logs are dropped on exit.

## CloudWatch EMF

For native AWS (Lambda or Fargate to CloudWatch), swap the metric formatter from `Json::new()` to `Emf::builder(namespace, dimensions).build()` (or `Emf::all_validations(..)` while developing) in `init_metrics`.

## dial9 (the `dial9` feature)

When enabled, dial9 records runtime telemetry through the `#[dial9::main]` runtime and a filtered tracing layer. Two wiring facts matter and are easy to break:

- It requires `--cfg tokio_unstable` and `-C force-frame-pointers=yes`, set in the generated `.cargo/config.toml`. Removing them makes dial9 fail to build or symbolize.
- The `Dial9TokioLayer` carries a `Targets` filter (this crate at TRACE, everything else at ERROR). Without aggressive filtering, SDK spans can flood the trace at over 100k events/s.

dial9 ships its own agent skills and a trace viewer. This skill does not re-explain trace or flamegraph reading; see the References.

## Invariants

- Every handler takes `HandlerMetricsGuard` and fills it; the middleware holds the parent `RequestMetricsGuard`, which emits the record on drop. Do not construct a second metric record per request.
- `/health` is mounted outside the telemetry layer and must stay there: probes should not emit per-request metrics.
- Keep the `TelemetryGuard` alive for the whole process; dropping it early stops log flushing and detaches the metric sink.
- Keep `--cfg tokio_unstable` in `.cargo/config.toml` while any metrics run: the tokio-metrics bridge depends on it. The `force-frame-pointers` flag is generated only with `dial9` and is needed only for dial9 stack symbolization.
- When `dial9` is on, spawn background tasks with `dial9::spawn` (not `tokio::spawn`) or they are invisible to the profiler.

## References

- metrique, including its `guide` module in the rustdoc: https://docs.rs/metrique
- dial9: https://github.com/dial9-rs/dial9-tokio-telemetry, viewer at https://dial9-tokio-telemetry.russell-r-cohen.workers.dev/ (dial9 also installs its own agent skills)
- Allocator profiling: see the `memory-allocator` skill.
