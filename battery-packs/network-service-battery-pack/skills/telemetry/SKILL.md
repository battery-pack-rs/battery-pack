---
name: telemetry
description: Observability wiring for the network-service scaffold: metrique wide-event metrics, tracing logs, and optional dial9 runtime profiling
---

# Telemetry

How this service emits metrics and logs, and how to read its runtime behavior. The wiring lives in `src/telemetry.rs` (init), `src/metrics.rs` (the metric records), and `src/middleware.rs` (the per-request recorder).

## Why one wide event per request

Every request emits a single `RequestMetrics` record carrying all of its metadata. One row per request means you answer questions ("p99 latency for SetItem when the downstream timed out") with a filter on one stream, not a join across separate counter and timer streams. Add a new property by adding a field to `RequestMetrics` or to the handler-owned `HandlerMetrics` slot, not by emitting a second record.

## metrique and dial9 do different jobs

This scaffold splits observability across two tools on purpose:

- **metrique** owns the business and RED metrics (rate, errors, duration) you alarm and dashboard on. It also bridges Tokio runtime metrics (queue depth, busy ratio) via `subscribe_tokio_runtime_metrics`, fully populated only when `tokio_unstable` is set (the `dial9` feature sets it); otherwise the bridge reports a reduced subset.
- **dial9** (optional, off by default) is an always-on Tokio profiler for root-cause work: poll timing, park/unpark, wake, per-task CPU. You reach for it when the metrics say "slow" but not "why".

## Where output goes

`--telemetry-dir` (env `TELEMETRY_DIR`) selects the destination at runtime. Unset (the default) sends logs to stderr and metrics to stdout on separate streams, which is what you want for local runs and container log capture. Set it to a directory and both roll into files there (`application.log` hourly, `metrics.log` minutely). Logs use a non-blocking writer so a slow or full disk never blocks request handling; the `TelemetryGuard` returned by `init_telemetry` must stay alive for the life of the process or buffered logs are dropped on exit.

## CloudWatch EMF

For native AWS (Lambda or Fargate to CloudWatch), swap the metric formatter in `init_metrics` from `Json::new()` to `Emf::builder(namespace, dimensions).build()` (or `Emf::all_validations(..)` while developing). See metrique's `examples/` directory for a complete EMF setup.

## dial9 (the `dial9` feature)

dial9 is off unless you build with the `dial9` feature, and even then does nothing until enabled at runtime.

- **Build flags:** the `dial9` feature adds `--cfg tokio_unstable` (runtime hooks) and `-C force-frame-pointers=yes` (stack symbolization) to `.cargo/config.toml`. Stripping them while dial9 is enabled breaks the build.
- **Tracing filter:** the `Dial9TokioLayer` carries a `Targets` filter (this crate at TRACE, everything else at ERROR). Without aggressive filtering, SDK spans can flood the trace with over 100k events/s.
- **Runtime config:** dial9 reads `DIAL9_*` env vars (`Dial9Config::from_env`). The generated `dial9.env` sets the common ones (`DIAL9_ENABLED=true`, `DIAL9_TRACE_DIR`, `DIAL9_CPU_PROFILE_ENABLED`, `DIAL9_MEMORY_PROFILE_ENABLED`); `source dial9.env` before running.
- **Trace destination:** local disk by default. On ephemeral compute (Fargate, Lambda) where local traces are lost on shutdown, enable the `worker-s3` feature and set `DIAL9_S3_BUCKET` to upload sealed segments to S3, or use `with_custom_pipeline` to ship them anywhere else.
- **CPU and schedule profiling (Linux)** need relaxed perf permissions (`kernel.perf_event_paranoid`); see dial9's docs for the exact sysctls.
- **Task dumps** (`DIAL9_TASK_DUMP_ENABLED`) capture an async backtrace of what each idle task is awaiting, useful for hangs. The `taskdump` feature compiles only on Linux (aarch64/x86/x86_64) and is a hard compile error elsewhere; it also adds an extra wake per capture, so measure before enabling on a hot path.

dial9 ships its own agent skills and a trace viewer that cover setup and analysis in depth, so this skill does not re-explain trace reading. To install the CLI, run `cargo install --locked dial9`. Then `dial9 serve` opens the trace viewer and `dial9 agents` provides its analysis skills. With Symposium, `cargo agents sync` auto-installs them.

## Invariants

- Every handler takes `HandlerMetricsGuard` and fills it; the middleware holds the parent `RequestMetricsGuard`, which emits the record on drop. Do not construct a second metric record per request.
- `/health` is mounted outside the telemetry layer. Probes should not emit per-request metrics.
- Keep the `TelemetryGuard` alive for the whole process; dropping it early stops log flushing and detaches the metric sink.
- The `dial9` feature adds `tokio_unstable` and frame pointers to `.cargo/config.toml`; do not strip them while dial9 is enabled, or dial9 will not build.
- When `dial9` is on, spawn background tasks with `dial9_tokio_telemetry::spawn` (not `tokio::spawn`) or they are invisible to the profiler.

## References

- metrique `_guide` module (longer-form docs): https://docs.rs/metrique/latest/metrique/_guide/
- metrique examples (including EMF): https://github.com/awslabs/metrique/tree/main/metrique/examples
- dial9: https://github.com/dial9-rs/dial9-tokio-telemetry (ships its own agent skills and viewer)
- Allocator profiling: see the `memory-allocator` skill.
