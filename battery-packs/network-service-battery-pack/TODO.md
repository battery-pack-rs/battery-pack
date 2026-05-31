# Ideas for future-work

- OTEL/OTLP metrics: once metrique ships an OpenTelemetry formatter, offer it alongside `Json::new()` / `Emf` in the metrics init
- metrique queue self-metrics (metrique_queue_overflows / queue_len / idle_percent): Currently these only surface via the metrics-rs bridge, which is not good enough to depend on yet. Revisit once improved: https://github.com/awslabs/metrique/issues/205. 
- metrique sysinfo integration: https://github.com/awslabs/metrique/issues/255
- dial9 heap profiling: `main.rs` installs the memory profiler programmatically via `MemoryProfiler::install`, gated on `DIAL9_MEMORY_PROFILE_ENABLED`, because `Dial9Config::from_env` does not wire that knob yet. Drop the manual install and let `from_env` handle it once https://github.com/dial9-rs/dial9/issues/457 lands.
- Redis, postgres, kafka, grpc
- Async-processor template (`templates/processor/`): a long-running consumer that pulls work from an input source and processes it (probably kafka, redis lists, maybe postgres?)
- Additional serialization formats beyond JSON (protobuf, maybe postcard, maybe flatc)
