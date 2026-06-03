# Ideas for future-work

- OTEL/OTLP metrics: once metrique ships an OpenTelemetry formatter, offer it alongside `Json::new()` / `Emf` in the metrics init (ie, start offering more turnkey formats besides json via placeholders)
- metrique queue self-metrics (metrique_queue_overflows / queue_len / idle_percent): Currently these only surface via the metrics-rs bridge, which is not good enough to depend on yet. Revisit once improved: https://github.com/awslabs/metrique/issues/205. 
- metrique sysinfo integration: https://github.com/awslabs/metrique/issues/255
- dial9 heap profiling: `main.rs` installs the memory profiler programmatically via `MemoryProfiler::install`, gated on `DIAL9_MEMORY_PROFILE_ENABLED`, because `Dial9Config::from_env` does not wire that knob yet. Drop the manual install and let `from_env` handle it once https://github.com/dial9-rs/dial9/issues/457 lands.
- axum-spawned task tracing: `axum::serve` spawns the per-connection and HTTP/2 tasks through its own executor, so they bypass `dial9::spawn` and their wake events never reach dial9 traces. Adopt `Serve::with_executor` to hand axum a dial9 executor once it ships in a released axum (merged in https://github.com/tokio-rs/axum/pull/3704, not in 0.8.9 yet); track dial9's native integration at https://github.com/dial9-rs/dial9/issues/152.
- More downstreams: redis, kafka, grpc
- Alternatives to axum: raw hyper/hyper-util, tonic (or grpc-rust)
- Async-processor template (`templates/processor/`): a long-running consumer that pulls work from an input source and processes it (probably kafka, redis lists, maybe postgres? I imagine people are interested in s3/dynamodb, but if we have those we should also add at least GCP equivalent)
- Additional serialization formats beyond JSON (protobuf, maybe postcard, maybe flatbuffers?)
- TLS and mTLS listeners
